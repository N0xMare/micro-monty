//! Nested-call arguments to JSON and nested results to host values, with
//! the bounds and messages of plak's Monty worker (`plakous-py-worker`'s
//! `codec.rs`).

use std::{fmt::Write as _, io};

use micro_monty::{HostArgs, HostError, HostErrorKind, HostValue, Unconverted};
use plakous_cell::CellValue;
use serde_json::{Map, Value};

/// Bound on the compact JSON encoding of a nested-call argument (plak's
/// own bound on a nested call's input).
pub(crate) const MAX_ARGUMENT_BYTES: usize = 1_048_576;
const ARGUMENT_TOO_LARGE: &str = "Python tool argument exceeds 1 MiB";
/// `CPython`'s default `sys.int_max_str_digits`.
const MAX_INT_DIGITS: usize = 4300;

/// How the engine converts host-call arguments for this worker: leniently,
/// so this codec decides every refusal in py-worker's order, and within the
/// same byte and depth budgets (the engine charges a lower bound of what
/// this codec charges, so it never stops first).
pub(crate) fn host_args(max_depth: usize) -> HostArgs {
    HostArgs {
        max_bytes: MAX_ARGUMENT_BYTES as u64,
        max_depth: u32::try_from(max_depth).unwrap_or(u32::MAX),
        lenient: true,
    }
}

/// Converts the single argument of a Python tool call to JSON.
///
/// `list` and `tuple` become arrays, `dict` (whose keys must be `str`) an
/// object, and every other non-JSON value its Python `repr()` string. The
/// compact encoding is at most 1 MiB and an `int` past 4300 digits is
/// refused, as is nesting deeper than `max_depth`.
pub(crate) fn call_to_json(argument: &HostValue, max_depth: usize) -> Result<Value, String> {
    let mut budget = MAX_ARGUMENT_BYTES;
    to_json(argument, 0, max_depth, &mut budget)
}

fn charge(budget: &mut usize, cost: usize) -> Result<(), String> {
    *budget = budget
        .checked_sub(cost)
        .ok_or_else(|| ARGUMENT_TOO_LARGE.to_owned())?;
    Ok(())
}

/// Counts the bytes `serde_json` writes without storing them.
struct EncodedLength(usize);

impl io::Write for EncodedLength {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// Writing into a counter cannot fail, and these values always serialize.
fn encoded_len(value: &Value) -> usize {
    let mut length = EncodedLength(0);
    let _ = serde_json::to_writer(&mut length, value);
    length.0
}

fn encoded_str_len(text: &str) -> usize {
    let mut length = EncodedLength(0);
    let _ = serde_json::to_writer(&mut length, text);
    length.0
}

fn charged_leaf(budget: &mut usize, json: Value) -> Result<Value, String> {
    charge(budget, encoded_len(&json))?;
    Ok(json)
}

fn int_limit_error() -> String {
    format!(
        "Python tool argument int exceeds the limit ({MAX_INT_DIGITS} digits) for integer string conversion"
    )
}

fn too_deep(max_depth: usize) -> String {
    format!("Python tool argument nests deeper than {max_depth} levels")
}

fn to_json(
    value: &HostValue,
    depth: usize,
    max_depth: usize,
    budget: &mut usize,
) -> Result<Value, String> {
    if depth > max_depth {
        return Err(too_deep(max_depth));
    }
    match value {
        HostValue::None => charged_leaf(budget, Value::Null),
        HostValue::Bool(flag) => charged_leaf(budget, Value::Bool(*flag)),
        HostValue::Int(int) => charged_leaf(budget, Value::from(*int)),
        // A non-finite float encodes as `null`, as `serde_json` does.
        HostValue::Float(float) => charged_leaf(budget, Value::from(*float)),
        HostValue::BigInt(digits) => {
            if digits.trim_start_matches('-').len() > MAX_INT_DIGITS {
                return Err(int_limit_error());
            }
            charged_leaf(budget, Value::String(digits.clone()))
        }
        HostValue::Str(text) => {
            charge(budget, encoded_str_len(text))?;
            Ok(Value::String(text.clone()))
        }
        // A tuple is a JSON array, as before `HostValue::Tuple` existed:
        // plak's `CellValue` has no tuple.
        HostValue::List(items) | HostValue::Tuple(items) => {
            charge(budget, 2 + items.len().saturating_sub(1))?;
            items
                .iter()
                .map(|item| to_json(item, depth + 1, max_depth, budget))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        }
        // The worker defines no host classes; an object would cross as a record does.
        HostValue::Record { fields, .. } | HostValue::Object { fields, .. } => {
            charge(budget, 2 + fields.len().saturating_sub(1))?;
            fields
                .iter()
                .map(|(_, item)| to_json(item, depth + 1, max_depth, budget))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        }
        HostValue::Dict(pairs) => {
            charge(budget, 2 + pairs.len().saturating_sub(1))?;
            let mut map = Map::new();
            for (key, item) in pairs {
                if matches!(item, HostValue::Unconverted(Unconverted::KeyNotStr)) {
                    return Err("Python tool argument dict keys must be str".to_owned());
                }
                // The key, its quotes and the separating colon.
                charge(budget, encoded_str_len(key) + 1)?;
                let item = to_json(item, depth + 1, max_depth, budget)?;
                map.insert(key.clone(), item);
            }
            Ok(Value::Object(map))
        }
        // Python's repr, as plak's py-worker sends it.
        HostValue::Bytes(bytes) => charged_leaf(budget, Value::String(bytes_repr(bytes))),
        HostValue::Unconverted(unconverted) => match unconverted {
            // Not a `str`, so `str()` is the value's `repr()`.
            Unconverted::Opaque { repr, .. } => charged_leaf(budget, Value::String(repr.clone())),
            Unconverted::KeyNotStr => Err("Python tool argument dict keys must be str".to_owned()),
            Unconverted::TooDeep => Err(too_deep(max_depth)),
            Unconverted::TooLarge { .. } => Err(ARGUMENT_TOO_LARGE.to_owned()),
            _ => Err("Python tool argument is not convertible".to_owned()),
        },
        _ => Err("Python tool argument is not convertible".to_owned()),
    }
}

fn bytes_repr(bytes: &[u8]) -> String {
    // Python's quote choice: `"` when the bytes hold `'` and no `"`.
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut out = String::from("b");
    out.push(char::from(quote));
    for &byte in bytes {
        match byte {
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            _ if byte == quote => {
                out.push('\\');
                out.push(char::from(byte));
            }
            0x20..=0x7e => out.push(char::from(byte)),
            _ => {
                let _ = write!(out, "\\x{byte:02x}");
            }
        }
    }
    out.push(char::from(quote));
    out
}

/// The value or failure a nested call resumes the cell with. A failure
/// raises `RuntimeError` with the host's diagnostic, bounded to
/// `max_error_bytes`. A value the interpreter cannot take (or nested deeper
/// than `max_depth`) is refused (`Err`): py-worker loses the worker over it.
pub(crate) fn nested_outcome(
    outcome: Result<CellValue, String>,
    max_depth: usize,
    max_error_bytes: usize,
) -> Result<Result<HostValue, HostError>, String> {
    match outcome {
        Ok(value) => cell_to_host(value, 0, max_depth).map(Ok),
        Err(reason) => Ok(Err(HostError {
            kind: HostErrorKind::RuntimeError,
            message: sanitize_nested_error(&reason, max_error_bytes),
            data: None,
            class: None,
        })),
    }
}

fn cell_to_host(value: CellValue, depth: usize, max_depth: usize) -> Result<HostValue, String> {
    if depth > max_depth {
        return Err(format!("cell result nests deeper than {max_depth} levels"));
    }
    let nested = |values: Vec<CellValue>| {
        values
            .into_iter()
            .map(|value| cell_to_host(value, depth + 1, max_depth))
            .collect::<Result<Vec<_>, _>>()
    };
    let fields = |fields: Vec<(String, CellValue)>| {
        fields
            .into_iter()
            .map(|(key, value)| Ok((key, cell_to_host(value, depth + 1, max_depth)?)))
            .collect::<Result<Vec<_>, String>>()
    };
    Ok(match value {
        CellValue::Null => HostValue::None,
        CellValue::Bool(flag) => HostValue::Bool(flag),
        CellValue::Int(int) => HostValue::Int(int),
        // py-worker reads a big integer through JSON: only one that fits
        // `i64` after all is taken.
        CellValue::BigInt(digits) => {
            let unsigned = digits
                .parse::<u64>()
                .map_err(|_| "cell integer has no JSON form".to_owned())?;
            HostValue::Int(
                i64::try_from(unsigned)
                    .map_err(|_| format!("unsupported JSON integer magnitude: {unsigned}"))?,
            )
        }
        CellValue::Float(float) if float.is_finite() => HostValue::Float(float),
        CellValue::Float(_) => return Err("cell float has no JSON form".into()),
        CellValue::Str(text) => HostValue::Str(text),
        CellValue::Bytes(bytes) => HostValue::Bytes(bytes),
        CellValue::List(values) => HostValue::List(nested(values)?),
        CellValue::Dict(entries) => HostValue::Dict(fields(entries)?),
        CellValue::Record {
            name,
            fields: entries,
        } => HostValue::Record {
            name,
            fields: fields(entries)?,
        },
    })
}

fn sanitize_nested_error(message: &str, max_bytes: usize) -> String {
    if message.len() <= max_bytes {
        return message.to_owned();
    }
    let mut end = max_bytes;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[diagnostic truncated]", &message[..end])
}
