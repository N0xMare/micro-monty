//! One-shot fault injection for the isolated, single-threaded QA process.

use core::sync::atomic::{AtomicUsize, Ordering::Relaxed};

static FAIL_AT: AtomicUsize = AtomicUsize::new(0);
static CHARGES: AtomicUsize = AtomicUsize::new(0);

/// Count positive, otherwise admissible charges. Refuse the nth one, or
/// count only when n is zero. Arm only around one session's cell.
pub fn arm(n: usize) {
    CHARGES.store(0, Relaxed);
    FAIL_AT.store(if n == 0 { usize::MAX } else { n }, Relaxed);
}

/// Stop injection and return the charge count for the cell.
pub fn disarm() -> usize {
    FAIL_AT.store(0, Relaxed);
    CHARGES.load(Relaxed)
}

pub(crate) fn reject(bytes: u64) -> bool {
    let at = FAIL_AT.load(Relaxed);
    let hit = at != 0 && bytes != 0 && CHARGES.fetch_add(1, Relaxed) == at - 1;
    if hit {
        refused();
    }
    hit
}

/// Runs only at the refused charge: a debugger's breakpoint, with the
/// charging site on the stack.
#[inline(never)]
pub fn refused() {
    core::hint::black_box(());
}
