//! The `mm-wasm` codec (`crates/mm-wasm-abi`, docs/spec.md §10.3):
//! every decoder on arbitrary bytes must return a value or a `Bad`, never
//! panic, overflow the stack or allocate beyond its input, and what decodes
//! must survive its own encoding: encode, decode the whole encoding, encode
//! again, same bytes (bytes, not `==`, so NaN floats compare).
//!
//! The first byte picks the decoder; the rest is its input.
//!
//! ```bash
//! cd fuzz && cargo +nightly fuzz run wasm_abi
//! ```

#![no_main]

use mm_wasm_abi::{Bad, Reader};

/// Decodes `data` with `get`, then checks the round trip with `put`.
fn round<T>(
    data: &[u8],
    get: impl Fn(&mut Reader<'_>) -> Result<T, Bad>,
    put: impl Fn(&mut Vec<u8>, &T),
) {
    let Ok(v) = get(&mut Reader::new(data)) else {
        return;
    };
    let mut once = Vec::new();
    put(&mut once, &v);
    let mut r = Reader::new(&once);
    let back = get(&mut r).expect("own encoding decodes");
    assert!(r.rest().is_empty(), "own encoding leaves bytes");
    let mut twice = Vec::new();
    put(&mut twice, &back);
    assert!(once == twice, "encoding is not stable");
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    use mm_wasm_abi as a;
    let Some((&pick, rest)) = data.split_first() else {
        return;
    };
    match pick % 18 {
        0 => round(rest, a::get_value, a::put_value),
        1 => round(rest, a::get_options, a::put_options),
        2 => round(rest, a::get_module, a::put_module),
        3 => round(rest, a::get_function_refs, |o, v| {
            a::put_function_refs(o, v)
        }),
        4 => round(rest, a::get_call_log, |o, v| a::put_call_log(o, v)),
        5 => round(rest, a::get_modules, |o, v| a::put_modules(o, v)),
        6 => round(rest, a::get_stubs, |o, v| a::put_stubs(o, v)),
        7 => round(rest, a::get_host_error, a::put_host_error),
        8 => round(rest, a::get_resume, a::put_resume),
        9 => round(rest, a::get_exception, a::put_exception),
        10 => round(rest, a::get_compile_error, a::put_compile_error),
        11 => round(rest, a::get_resume_error, a::put_resume_error),
        12 => round(rest, a::get_snapshot_error, a::put_snapshot_error),
        13 => {
            // A progress is its event code (the export's return), then the payload.
            let Some((code, payload)) = rest.split_first_chunk::<4>() else {
                return;
            };
            let code = u32::from_le_bytes(*code);
            let Ok(p) = a::get_progress(code, &mut Reader::new(payload)) else {
                return;
            };
            let mut once = Vec::new();
            let c1 = a::put_progress(&mut once, &p);
            let mut r = Reader::new(&once);
            let back = a::get_progress(c1, &mut r).expect("own encoding decodes");
            assert!(r.rest().is_empty(), "own encoding leaves bytes");
            let mut twice = Vec::new();
            let c2 = a::put_progress(&mut twice, &back);
            assert!(c1 == c2 && once == twice, "encoding is not stable");
        }
        14 => {
            if let Some(code) = rest.first_chunk::<4>() {
                let code = u32::from_le_bytes(*code);
                if let Ok(cut) = a::text_cut_from_code(code) {
                    assert_eq!(a::text_cut_code(cut), code);
                }
            }
        }
        15 => {
            if let Ok(key) = std::str::from_utf8(rest) {
                let _ = a::hint_group(key);
            }
        }
        16 => round(rest, a::get_define_error, a::put_define_error),
        _ => {
            if let Some(code) = rest.first_chunk::<4>() {
                let code = u32::from_le_bytes(*code);
                if let Ok(phase) = a::phase_from_code(code) {
                    assert_eq!(a::phase_code(phase), code);
                }
            }
        }
    }
});
