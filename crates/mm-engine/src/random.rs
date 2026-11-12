//! `random`'s generator: MT19937 as CPython's `Modules/_randommodule.c`
//! runs it (`init_by_array` seeding, `genrand_res53`, `getrandbits`), so a
//! seeded sequence matches CPython's draw for draw. The state lives in an
//! arena `bytes` object owned by the `Random` instance (624 words and the
//! index, little-endian), mutated in place, so it is snapshotted with the
//! rest of the heap. `random.py` holds the API (docs/spec.md §12).

use crate::types::T;
use crate::vm::{CallArgs, Raised, Vm};
use alloc::vec::Vec;
use mm_core::Value;

const N: usize = 624;
const M: usize = 397;
/// Bytes of a state: the words and the index.
const STATE_BYTES: usize = (N + 1) * 4;

/// A generator state, unpacked.
struct Mt {
    mt: [u32; N],
    index: usize,
}

impl Mt {
    #[cfg_attr(mm_shipped, optimize(size))]
    fn load(b: &[u8]) -> Option<Mt> {
        if b.len() != STATE_BYTES {
            return None;
        }
        let mut mt = [0u32; N];
        for (w, c) in mt.iter_mut().zip(b.as_chunks::<4>().0) {
            *w = u32::from_le_bytes(*c);
        }
        let index = u32::from_le_bytes([b[N * 4], b[N * 4 + 1], b[N * 4 + 2], b[N * 4 + 3]]);
        Some(Mt {
            mt,
            index: (index as usize).min(N),
        })
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn store(&self, b: &mut [u8]) {
        for (c, w) in b.as_chunks_mut::<4>().0.iter_mut().zip(self.mt.iter()) {
            c.copy_from_slice(&w.to_le_bytes());
        }
        if let Some(c) = b.get_mut(N * 4..N * 4 + 4) {
            c.copy_from_slice(&(self.index as u32).to_le_bytes());
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn init_genrand(&mut self, s: u32) {
        self.mt[0] = s;
        for i in 1..N {
            let p = self.mt[i - 1];
            self.mt[i] = 1_812_433_253u32
                .wrapping_mul(p ^ (p >> 30))
                .wrapping_add(i as u32);
        }
        self.index = N;
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn init_by_array(&mut self, key: &[u32]) {
        self.init_genrand(19_650_218);
        let (mut i, mut j) = (1usize, 0usize);
        let len = key.len().max(1);
        for _ in 0..N.max(len) {
            let p = self.mt[i - 1];
            self.mt[i] = (self.mt[i] ^ (p ^ (p >> 30)).wrapping_mul(1_664_525))
                .wrapping_add(key.get(j).copied().unwrap_or(0))
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
            if j >= len {
                j = 0;
            }
        }
        for _ in 0..N - 1 {
            let p = self.mt[i - 1];
            self.mt[i] =
                (self.mt[i] ^ (p ^ (p >> 30)).wrapping_mul(1_566_083_941)).wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
        }
        self.mt[0] = 0x8000_0000;
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn next_u32(&mut self) -> u32 {
        const UPPER: u32 = 0x8000_0000;
        const LOWER: u32 = 0x7fff_ffff;
        const MATRIX_A: u32 = 0x9908_b0df;
        let mag = |y: u32| if y & 1 == 0 { 0 } else { MATRIX_A };
        if self.index >= N {
            for kk in 0..N - M {
                let y = (self.mt[kk] & UPPER) | (self.mt[kk + 1] & LOWER);
                self.mt[kk] = self.mt[kk + M] ^ (y >> 1) ^ mag(y);
            }
            for kk in N - M..N - 1 {
                let y = (self.mt[kk] & UPPER) | (self.mt[kk + 1] & LOWER);
                self.mt[kk] = self.mt[kk + M - N] ^ (y >> 1) ^ mag(y);
            }
            let y = (self.mt[N - 1] & UPPER) | (self.mt[0] & LOWER);
            self.mt[N - 1] = self.mt[M - 1] ^ (y >> 1) ^ mag(y);
            self.index = 0;
        }
        let mut y = self.mt[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn random(&mut self) -> f64 {
        let a = self.next_u32() >> 5;
        let b = self.next_u32() >> 6;
        (f64::from(a) * 67_108_864.0 + f64::from(b)) * (1.0 / 9_007_199_254_740_992.0)
    }

    /// `getrandbits(k)` for `k <= 64`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn bits64(&mut self, k: u32) -> u64 {
        if k == 0 {
            return 0;
        }
        if k <= 32 {
            return u64::from(self.next_u32() >> (32 - k));
        }
        let lo = u64::from(self.next_u32());
        let hi = u64::from(self.next_u32() >> (64 - k));
        lo | (hi << 32)
    }
}

/// Runs `f` on the state in `st` (a `bytes` of the right size), writing
/// it back afterwards.
#[cfg_attr(mm_shipped, optimize(size))]
fn with_state<R>(vm: &mut Vm, st: Value, f: impl FnOnce(&mut Mt) -> R) -> Result<R, Raised> {
    let b = if Vm::is_bytes(st) {
        vm.vslot_o(st, 0).as_byte_ref()
    } else {
        None
    };
    let Some(b) = b else {
        return Err(vm.err(T::TypeError, "random: bad generator state"));
    };
    let Some(mut mt) = vm.heap.bytes(b).ok().and_then(Mt::load) else {
        return Err(vm.err(T::TypeError, "random: bad generator state"));
    };
    let r = f(&mut mt);
    if let Ok(data) = vm.heap.bytes_mut(b) {
        mt.store(data);
    }
    Ok(r)
}

/// `_mt_new()`: a fresh (unseeded) state.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mt_new(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    let mut mt = Mt {
        mt: [0; N],
        index: N,
    };
    mt.init_genrand(5489);
    let mut b = alloc::vec![0u8; STATE_BYTES];
    mt.store(&mut b);
    vm.new_bytes(&b)
}

/// `_mt_seed(state, key)`: `init_by_array` with `key` (bytes, the seed's
/// magnitude little-endian; empty for 0).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mt_seed(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let key_v = vm.arg_o(&a, 1);
    if !Vm::is_bytes(key_v) {
        return Err(vm.err(T::TypeError, "random: the key must be bytes"));
    }
    let data = vm.bytes_data(key_v);
    let mut key: Vec<u32> = Vec::with_capacity(data.len() / 4 + 1);
    for c in data.chunks(4) {
        let mut w = [0u8; 4];
        w[..c.len()].copy_from_slice(c);
        key.push(u32::from_le_bytes(w));
    }
    while key.len() > 1 && key.last() == Some(&0) {
        key.pop();
    }
    if key.is_empty() {
        key.push(0);
    }
    vm.burn(key.len() as u64 / 16);
    with_state(vm, vm.arg_o(&a, 0), |mt| mt.init_by_array(&key))?;
    Ok(Value::NONE)
}

/// `_mt_random(state)`: a float in [0, 1) (`genrand_res53`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mt_random(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = with_state(vm, vm.arg_o(&a, 0), Mt::random)?;
    Ok(Value::float(x))
}

/// `_mt_bits(state, k)`: `getrandbits(k)` (`k >= 0` checked by the
/// caller): an int for `k <= 64`, else the words as little-endian bytes.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mt_bits(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let k = vm.int_arg(vm.arg_o(&a, 1), "")?;
    let Ok(k) = u64::try_from(k) else {
        return Err(vm.err(T::ValueError, "Cannot convert negative int"));
    };
    if k <= 64 {
        let x = with_state(vm, vm.arg_o(&a, 0), |mt| mt.bits64(k as u32))?;
        return vm.int_from_i128(i128::from(x));
    }
    let words = (k - 1) / 32 + 1;
    let Ok(nbytes) = usize::try_from(words * 4) else {
        return Err(vm.err(T::MemoryError, ""));
    };
    if !vm.heap.would_fit(words * 4 * 3 + 64) {
        return Err(vm.err(T::MemoryError, ""));
    }
    vm.burn(words / 8);
    let out = with_state(vm, vm.arg_o(&a, 0), |mt| {
        let mut out = Vec::with_capacity(nbytes);
        let mut left = k;
        while left > 0 {
            let mut r = mt.next_u32();
            if left < 32 {
                r >>= 32 - left;
            }
            out.extend_from_slice(&r.to_le_bytes());
            left = left.saturating_sub(32);
        }
        out
    })?;
    vm.new_bytes(&out)
}

/// `_mt_below(state, n)`: `_randbelow_with_getrandbits(n)` for
/// `0 < n < 2**64`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mt_below(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let n = vm.int_arg(vm.arg_o(&a, 1), "")?;
    let Ok(n) = u64::try_from(n) else {
        return Err(vm.err(T::ValueError, "random: bad bound"));
    };
    if n == 0 {
        return vm.new_int(0);
    }
    let k = 64 - n.leading_zeros();
    let r = with_state(vm, vm.arg_o(&a, 0), |mt| {
        loop {
            let r = mt.bits64(k);
            if r < n {
                break r;
            }
        }
    })?;
    vm.int_from_i128(i128::from(r))
}

/// `_mt_getstate(state)`: the 624 words and the index, as a tuple.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mt_getstate(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (words, index) = with_state(vm, vm.arg_o(&a, 0), |mt| (mt.mt, mt.index))?;
    let mut items = Vec::with_capacity(N + 1);
    for w in words {
        items.push(vm.new_int(i64::from(w))?);
    }
    items.push(vm.new_int(index as i64)?);
    vm.alloc_tuple(&items)
}

/// `_mt_setstate(state, words)`: 625 ints, already checked (each word
/// below 2**64, truncated to 32 bits; the index in 0..=624).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mt_setstate(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let Some(items) = vm.seq_items(vm.arg_o(&a, 1)) else {
        return Err(vm.err(T::TypeError, "state vector must be a tuple"));
    };
    if items.len() != N + 1 {
        return Err(vm.err(T::ValueError, "state vector is the wrong size"));
    }
    let mut vals = [0u32; N + 1];
    for (d, v) in vals.iter_mut().zip(&items) {
        let Some(x) = vm.int_of_o(*v) else {
            return Err(vm.err(T::TypeError, "an integer is required"));
        };
        *d = x as u32;
    }
    with_state(vm, vm.arg_o(&a, 0), |mt| {
        mt.mt.copy_from_slice(&vals[..N]);
        mt.index = (vals[N] as usize).min(N);
    })?;
    Ok(Value::NONE)
}
