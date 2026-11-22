// The reset probes (README.md here), shared by the native facade's test
// (crates/micro-monty/tests/reset_boundary.rs) and the mm-wasm host's
// (crates/mm-wasm-host/tests/reset_boundary.rs), which include this file.
// hosts/js/test/reset.test.ts defines the same host and reads the same
// probes.

use micro_monty::{
    HostClass, HostError, HostErrorKind, HostException, HostModule, HostParam, HostSignature,
    HostValue, ParamKind, Progress, Resume,
};
use std::sync::{Arc, Mutex};

/// A session of the host under test.
pub trait Cells {
    /// Starts a cell.
    fn start(&mut self, source: &str) -> Progress;
    /// Answers the host call it stopped at.
    fn resume(&mut self, answer: Resume) -> Progress;
    /// `Session::reset`.
    fn reset(&mut self);
}

fn sig(name: &str, params: Vec<HostParam>) -> HostSignature {
    HostSignature {
        name: name.to_owned(),
        params,
        returns: "object".to_owned(),
        doc: String::new(),
        dict_as_kwargs: false,
    }
}

fn param(name: &str, default: HostValue, kind: ParamKind) -> HostParam {
    HostParam {
        name: name.to_owned(),
        annotation: String::new(),
        default: Some(default),
        kind,
    }
}

fn row() -> HostValue {
    HostValue::Record {
        name: "Row".to_owned(),
        fields: vec![("a".to_owned(), HostValue::Int(1))],
    }
}

/// The host's definitions: typed and untyped functions, a class, an
/// exception class, constants of every mutable kind and record defaults
/// in the builtins, and the same in a module.
pub fn modules() -> Vec<HostModule> {
    let list = |v: &str| HostValue::List(vec![HostValue::Str(v.to_owned())]);
    let class = |name: &str, methods| HostClass {
        name: name.to_owned(),
        fields: vec![("path".to_owned(), "str".to_owned())],
        methods,
        doc: String::new(),
    };
    let exception = |name: &str| HostException {
        name: name.to_owned(),
        base: HostErrorKind::RuntimeError,
        doc: String::new(),
    };
    let read = sig(
        "read",
        vec![
            param("paths", list("/safe"), ParamKind::PositionalOrKeyword),
            param("limit", HostValue::Int(1), ParamKind::KeywordOnly),
        ],
    );
    let go = sig(
        "go",
        vec![param(
            "n",
            HostValue::Int(1),
            ParamKind::PositionalOrKeyword,
        )],
    );
    // Record defaults: instances of the record class, which the functions
    // keep through every reset.
    let record = |name: &str| {
        sig(
            name,
            vec![
                param("row", row(), ParamKind::PositionalOrKeyword),
                param("rows", HostValue::List(vec![row()]), ParamKind::KeywordOnly),
            ],
        )
    };
    vec![
        HostModule {
            name: String::new(),
            functions: vec![
                sig("h", Vec::new()),
                sig("rec", Vec::new()),
                sig("obj", Vec::new()),
                sig("boom", Vec::new()),
                read.clone(),
                record("readr"),
                HostSignature::any("tool"),
            ],
            classes: vec![class("Entry", vec![go, record("gor")])],
            constants: vec![
                ("CONF".to_owned(), list("a")),
                (
                    "CONFD".to_owned(),
                    HostValue::Dict(vec![("a".to_owned(), list("b"))]),
                ),
                ("CONFR".to_owned(), row()),
            ],
            exceptions: vec![exception("ToolError")],
        },
        HostModule {
            name: "fs".to_owned(),
            functions: vec![read, HostSignature::any("raw")],
            classes: vec![class("Node", Vec::new())],
            constants: vec![("ROOTS".to_owned(), list("/"))],
            exceptions: vec![exception("FsError")],
        },
    ]
}

/// The host's answer to a call; `None` leaves the cell stopped there
/// (`tool('stop')`), for the reset to end.
fn answer(name: &str, args: &[HostValue]) -> Option<Resume> {
    Some(match name {
        "tool" if args == [HostValue::Str("stop".to_owned())] => return None,
        "rec" => Resume::Value(row()),
        "obj" => Resume::Value(HostValue::Object {
            class: "Entry".to_owned(),
            fields: vec![("path".to_owned(), HostValue::Str("p".to_owned()))],
        }),
        "boom" => {
            let mut e = HostError::new(HostErrorKind::RuntimeError, "bad");
            e.class = Some("ToolError".to_owned());
            Resume::Error(e)
        }
        _ => Resume::Value(HostValue::None),
    })
}

/// Runs a cell: how it ended (`Type: message`, its value's repr or
/// `stopped`), then its host calls and its output.
fn cell(s: &mut dyn Cells, out: &Arc<Mutex<String>>, source: &str) -> (String, String) {
    out.lock().expect("output lock").clear();
    let mut calls = Vec::new();
    let mut p = s.start(source);
    let end = loop {
        match p {
            Progress::HostCall { name, args, .. } => {
                calls.push(format!("{name}{args:?}"));
                match answer(&name, &args) {
                    Some(a) => p = s.resume(a),
                    None => break "stopped".to_owned(),
                }
            }
            Progress::Complete(Some(c)) => break c.repr,
            Progress::Complete(None) => break "None".to_owned(),
            Progress::Error(e) => break format!("{}: {}", e.type_name, e.message),
            other => break format!("{other:?}"),
        }
    };
    let output = out.lock().expect("output lock").clone();
    (end.clone(), format!("{end}\ncalls {calls:?}\n{output}"))
}

/// Makes a session with [`modules`] defined, its output going to the buffer.
pub type New<'a> = &'a dyn Fn(&Arc<Mutex<String>>) -> Box<dyn Cells>;

/// A probe of probes.txt.
pub struct Probe {
    pub label: String,
    /// A documented difference (`!==`).
    pub differs: bool,
    /// What cell A must end in (`>>>`).
    pub a_ends: Option<String>,
    pub a: String,
    pub b: String,
}

/// The probes of `text` (probes.txt), `@walk` and `@sweep` lines replaced
/// by `walk` (walk.py) and `sweep` (sweep.py) in that mode.
pub fn probes(text: &str, walk: &str, sweep: &str) -> Vec<Probe> {
    let mut probes: Vec<Probe> = Vec::new();
    let mut in_b = false;
    for line in text.lines() {
        if let Some(label) = line.strip_prefix("=== ").or(line.strip_prefix("!== ")) {
            probes.push(Probe {
                label: label.to_owned(),
                differs: line.starts_with('!'),
                a_ends: None,
                a: String::new(),
                b: String::new(),
            });
            in_b = false;
            continue;
        }
        let Some(p) = probes.last_mut() else { continue };
        if line == "---" {
            in_b = true;
        } else if let Some(ends) = line.strip_prefix(">>> ") {
            p.a_ends = Some(ends.to_owned());
        } else {
            let part = if in_b { &mut p.b } else { &mut p.a };
            let script = line
                .strip_prefix("@walk ")
                .map(|mode| (mode, walk))
                .or(line.strip_prefix("@sweep ").map(|mode| (mode, sweep)));
            if let Some((mode, script)) = script {
                part.push_str("MODE = '");
                part.push_str(mode);
                part.push_str("'\n");
                part.push_str(script);
            } else {
                part.push_str(line);
                part.push('\n');
            }
        }
    }
    probes
}

/// Every probe on sessions `new` makes: cell B after cell A and a reset reports
/// what B reports in a new session. Returns how many probes ran.
pub fn check(probes: &[Probe], new: New<'_>) -> Result<usize, String> {
    let mut failures = Vec::new();
    for p in probes {
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = new(&out);
        let (a_end, _) = cell(s.as_mut(), &out, &p.a);
        if let Some(ends) = &p.a_ends
            && *ends != a_end
        {
            failures.push(format!(
                "{}: cell A ended in\n  {a_end}\nnot\n  {ends}",
                p.label
            ));
        }
        s.reset();
        let (_, after) = cell(s.as_mut(), &out, &p.b);
        let out = Arc::new(Mutex::new(String::new()));
        let mut fresh = new(&out);
        let (_, expected) = cell(fresh.as_mut(), &out, &p.b);
        if (after == expected) == p.differs {
            failures.push(format!(
                "{}: after cell A and a reset, cell B reports\n{after}\nand in a new session\n{expected}",
                p.label
            ));
        }
    }
    if failures.is_empty() {
        Ok(probes.len())
    } else {
        Err(failures.join("\n\n"))
    }
}

/// The walk's control: in one session, without a reset, a check after a
/// mark reports marks, so the walk reaches and writes objects. Returns
/// the check's output.
pub fn walk_control(walk: &str, new: New<'_>) -> String {
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = new(&out);
    cell(s.as_mut(), &out, &format!("MODE = 'mark'\n{walk}"));
    cell(s.as_mut(), &out, &format!("MODE = 'check'\n{walk}")).1
}
