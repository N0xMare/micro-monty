//! Checks [`mm_wasm_alloc::Allocator`] where it departs from std's calls:
//! `realloc` of blocks aligned above dlmalloc's natural `2 * usize`, which
//! must keep the alignment and the contents while growing and shrinking.
//! `Vec::push` grows and `shrink_to_fit` shrinks through
//! `GlobalAlloc::realloc`; allocations in between make the blocks move.
//! `cargo xtask wasm` runs it under wasmtime on `wasm32-unknown-unknown`
//! (exit code 0); on other targets std's allocator runs.

use std::hint::black_box;
use std::process::ExitCode;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[global_allocator]
static ALLOCATOR: mm_wasm_alloc::Allocator = mm_wasm_alloc::Allocator::new();

#[derive(Clone, Copy, PartialEq)]
#[repr(align(16))]
struct A16(u32);

#[derive(Clone, Copy, PartialEq)]
#[repr(align(64))]
struct A64(u32);

#[derive(Clone, Copy, PartialEq)]
#[repr(align(4096))]
struct A4096(u32);

/// Whether `v` is `align`-aligned and holds `make(0)..make(len)`. The
/// address goes through `black_box`: the compiler may otherwise assume a
/// `[T]` is aligned and fold the test away.
fn intact<T: Copy + PartialEq>(v: &[T], make: fn(u32) -> T, align: usize) -> bool {
    black_box(v.as_ptr().addr()).is_multiple_of(align) && (0..).zip(v).all(|(i, x)| *x == make(i))
}

fn check<T: Copy + PartialEq>(make: fn(u32) -> T, align: usize, n: u32) -> bool {
    let mut v: Vec<T> = Vec::with_capacity(1);
    let mut neighbours = Vec::new();
    for i in 0..n {
        v.push(make(i));
        neighbours.push(vec![i.to_le_bytes()[0]; 24 + v.len()]);
        if !intact(&v, make, align) {
            return false;
        }
    }
    for keep in [n / 2, 3, 1] {
        v.truncate(keep as usize);
        v.shrink_to_fit();
        neighbours.push(vec![0; 40]);
        if v.capacity() != v.len() || !intact(&v, make, align) {
            return false;
        }
    }
    v.extend((1..n).map(make));
    intact(&v, make, align)
}

fn main() -> ExitCode {
    let ok = check(A16, 16, 400) && check(A64, 64, 300) && check(A4096, 4096, 40);
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
