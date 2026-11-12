//! Wasmtime mode: the shared service as a `wasm32-wasip1` module.
//!
//! The ABI is two exports. `mm_in(len)` sizes the input buffer and returns
//! its address; the host writes an `iso_service` message there. `mm_call(len)`
//! handles it and returns the response's address and length packed as
//! `addr << 32 | len`. The buffers are zero-initialised `Vec`s the host
//! overwrites between calls, so the guest needs no `unsafe` to read them;
//! the export attributes are the only unsafe syntax (edition 2024).

use std::cell::RefCell;

use iso_service::{Request, Response, Service, decode, encode};

thread_local! {
    static SERVICE: RefCell<Service> = RefCell::new(Service::new());
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Sizes the input buffer to `len` bytes and returns its address.
#[unsafe(no_mangle)]
pub extern "C" fn mm_in(len: u32) -> u32 {
    INPUT.with_borrow_mut(|input| {
        input.clear();
        input.resize(len as usize, 0);
        input.as_ptr() as usize as u32
    })
}

/// Handles the `len`-byte message in the input buffer.
#[unsafe(no_mangle)]
pub extern "C" fn mm_call(len: u32) -> u64 {
    let message = INPUT.with_borrow(|input| {
        let body = input.get(..len as usize).unwrap_or_default();
        match decode::<Request>(body) {
            Ok((request, raw)) => {
                let (response, raw) = SERVICE.with_borrow_mut(|s| s.handle(&request, raw));
                encode(&response, &raw)
            }
            Err(message) => encode(&Response::Failed { message }, &[]),
        }
    });
    OUTPUT.with_borrow_mut(|output| {
        *output = message;
        (u64::from(output.as_ptr() as usize as u32) << 32) | output.len() as u64
    })
}
