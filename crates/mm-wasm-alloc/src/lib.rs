//! The global allocator for shipped `wasm32-unknown-unknown` modules: std's
//! own dlmalloc (the same crate and version), called through its layout-free
//! `c_free`/`c_realloc` entry points. std's allocator calls `free`/`realloc`
//! with the layout, which first runs `validate_size` (two `assert!`s on the
//! chunk header); under fat LTO and `panic=immediate-abort` those inline into
//! every deallocation site: 83,553 bytes of `hello.wasm` and 97,112 of the
//! guest [M] (docs/spec.md §6). The layout is still a precondition of
//! `GlobalAlloc`; it is only not re-checked.
//!
//! Binaries register it (the guest, the facade's `hello` and `roundtrip`
//! examples; optional for embedders, docs/user-guide.md), never the facade. On
//! every other target this crate is empty. Temporary (docs/spec.md §5):
//! delete it once a pinned stable std ships dlmalloc with the check as a
//! debug assertion.
#![no_std]

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use imp::Allocator;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[allow(unsafe_code, reason = "the GlobalAlloc impl; docs/spec.md §5")]
mod imp {
    use core::alloc::{GlobalAlloc, Layout};
    use core::cell::UnsafeCell;
    use core::ptr;

    use dlmalloc::Dlmalloc;

    /// dlmalloc's natural alignment, `2 * size_of::<usize>()`.
    const NATURAL: usize = 2 * size_of::<usize>();

    /// The allocator. Register it with `#[global_allocator]`.
    pub struct Allocator(UnsafeCell<Dlmalloc>);

    // SAFETY: without the `atomics` target feature this target has one
    // thread, so the allocator is never entered concurrently; it is not
    // reentrant either (dlmalloc does not allocate). std's wasm allocator
    // relies on the same.
    #[cfg(not(target_feature = "atomics"))]
    unsafe impl Sync for Allocator {}

    impl Allocator {
        /// An empty heap that grows with `memory.grow`, as std's.
        #[must_use]
        pub const fn new() -> Self {
            Self(UnsafeCell::new(Dlmalloc::new()))
        }
    }

    impl Default for Allocator {
        fn default() -> Self {
            Self::new()
        }
    }

    // SAFETY: each method forwards to the dlmalloc call std's
    // `sys::alloc::wasm` makes, under `GlobalAlloc`'s preconditions, except
    // that `dealloc`/`realloc` use the layout-free C entry points, which
    // accept any pointer this allocator returned.
    unsafe impl GlobalAlloc for Allocator {
        #[inline]
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            unsafe { (*self.0.get()).malloc(layout.size(), layout.align()) }
        }

        #[inline]
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            unsafe { (*self.0.get()).calloc(layout.size(), layout.align()) }
        }

        #[inline]
        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            unsafe { (*self.0.get()).c_free(ptr) }
        }

        #[inline]
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let heap = unsafe { &mut *self.0.get() };
            if layout.align() <= NATURAL {
                return unsafe { heap.c_realloc(ptr, new_size) };
            }
            // Over-aligned: `c_realloc` would drop the alignment.
            let new = unsafe { heap.malloc(new_size, layout.align()) };
            if !new.is_null() {
                unsafe {
                    ptr::copy_nonoverlapping(ptr, new, layout.size().min(new_size));
                    heap.c_free(ptr);
                }
            }
            new
        }
    }
}
