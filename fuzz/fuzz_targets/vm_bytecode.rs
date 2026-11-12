//! The VM on mutated bytecode (docs/spec.md §13): a few compiled programs
//! are encoded (`Program::encode`), the input edits the bytes, and what
//! still decodes and passes `Code::verify` (the check restored snapshots
//! get) runs with a fuel budget. Nothing may panic, overflow the stack or
//! run past its fuel; exceptions are fine.
//!
//! Input: byte 0 picks the program, then 4-byte edits: a 16-bit position,
//! a xor byte, and a byte whose value 0xFF truncates there instead.
//!
//! ```bash
//! cd fuzz && cargo +nightly fuzz run vm_bytecode
//! ```

#![no_main]

#[path = "vm_bytecode_input.rs"]
mod input;

libfuzzer_sys::fuzz_target!(|data: &[u8]| input::run(data));
