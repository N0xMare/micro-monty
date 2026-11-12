//! `hashlib`'s digests, written from their specifications: MD5 (RFC 1321),
//! SHA-1 and SHA-2 (FIPS 180-4), SHA-3 and SHAKE (FIPS 202), BLAKE2b/2s
//! (RFC 7693). A hasher's state lives in an arena `bytes` object owned by
//! the Python object (`hashlib.py` holds the API and CPython's argument
//! checks), mutated in place, so it is snapshotted with the heap.

#![allow(clippy::unreadable_literal, reason = "published constant tables")]

use crate::bytes::{Job, R_BYTES, W_END, W_O, job};
use crate::types::T;
use crate::vm::{CallArgs, Raised, Vm};
use alloc::vec::Vec;
use mm_core::Value;

const MD5_K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const K512: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

const IV256: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

const IV224: [u32; 8] = [
    0xc1059ed8, 0x367cd507, 0x3070dd17, 0xf70e5939, 0xffc00b31, 0x68581511, 0x64f98fa7, 0xbefa4fa4,
];

const IV512: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

const IV384: [u64; 8] = [
    0xcbbb9d5dc1059ed8,
    0x629a292a367cd507,
    0x9159015a3070dd17,
    0x152fecd8f70e5939,
    0x67332667ffc00b31,
    0x8eb44a8768581511,
    0xdb0c2e0d64f98fa7,
    0x47b5481dbefa4fa4,
];

const KECCAK_RC: [u64; 24] = [
    0x0000000000000001,
    0x0000000000008082,
    0x800000000000808a,
    0x8000000080008000,
    0x000000000000808b,
    0x0000000080000001,
    0x8000000080008081,
    0x8000000000008009,
    0x000000000000008a,
    0x0000000000000088,
    0x0000000080008009,
    0x000000008000000a,
    0x000000008000808b,
    0x800000000000008b,
    0x8000000000008089,
    0x8000000000008003,
    0x8000000000008002,
    0x8000000000000080,
    0x000000000000800a,
    0x800000008000000a,
    0x8000000080008081,
    0x8000000000008080,
    0x0000000080000001,
    0x8000000080008008,
];

const MD5_S: [u32; 16] = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
const SHA1_IV: [u32; 5] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];
const KECCAK_ROT: [u32; 24] = [
    1, 3, 6, 10, 15, 21, 28, 36, 45, 55, 2, 14, 27, 41, 56, 8, 25, 43, 62, 18, 39, 61, 20, 44,
];
const KECCAK_PI: [usize; 24] = [
    10, 7, 11, 17, 18, 3, 5, 16, 8, 21, 24, 4, 15, 23, 19, 13, 12, 2, 20, 14, 22, 9, 6, 1,
];
const SIGMA: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

/// Algorithm numbers (`hashlib.py` uses the same).
const MD5: u8 = 0;
const SHA1: u8 = 1;
const SHA224: u8 = 2;
const SHA256: u8 = 3;
const SHA384: u8 = 4;
const SHA512: u8 = 5;
const SHA3_224: u8 = 6;
const SHAKE128: u8 = 10;
const SHAKE256: u8 = 11;
const B2B: u8 = 12;
const B2S: u8 = 13;

/// Largest block (SHAKE128's rate).
const MAX_BLOCK: usize = 168;
/// Serialized state: alg, digest size, last-node flag, buffered count
/// (2), total length (16), 25 words (200), the buffer.
const STATE_BYTES: usize = 5 + 16 + 200 + MAX_BLOCK;

#[derive(Clone)]
struct St {
    alg: u8,
    out: u8,
    last: u8,
    n: usize,
    len: u128,
    w: [u64; 25],
    buf: [u8; MAX_BLOCK],
}

#[cfg_attr(mm_shipped, optimize(size))]
fn is_keccak(alg: u8) -> bool {
    (SHA3_224..=SHAKE256).contains(&alg)
}

/// Block size (the rate for Keccak).
#[cfg_attr(mm_shipped, optimize(size))]
fn block(alg: u8) -> usize {
    match alg {
        SHA384 | SHA512 | B2B => 128,
        6 => 144,
        7 | SHAKE256 => 136,
        8 => 104,
        9 => 72,
        SHAKE128 => 168,
        _ => 64,
    }
}

/// Default digest size (0 for SHAKE).
#[cfg_attr(mm_shipped, optimize(size))]
fn digest_size(alg: u8) -> u8 {
    [16, 20, 28, 32, 48, 64, 28, 32, 48, 64, 0, 0, 64, 32]
        .get(alg as usize)
        .copied()
        .unwrap_or(0)
}

impl St {
    #[cfg_attr(mm_shipped, optimize(size))]
    fn new(alg: u8) -> St {
        let mut w = [0u64; 25];
        match alg {
            MD5 | SHA1 => {
                for (d, s) in w.iter_mut().zip(SHA1_IV) {
                    *d = u64::from(s);
                }
            }
            SHA224 | SHA256 | B2S => {
                let iv = if alg == SHA224 { IV224 } else { IV256 };
                for (d, s) in w.iter_mut().zip(iv) {
                    *d = u64::from(s);
                }
            }
            SHA384 => w[..8].copy_from_slice(&IV384),
            SHA512 | B2B => w[..8].copy_from_slice(&IV512),
            _ => {}
        }
        St {
            alg,
            out: digest_size(alg),
            last: 0,
            n: 0,
            len: 0,
            w,
            buf: [0; MAX_BLOCK],
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn load(b: &[u8]) -> Option<St> {
        if b.len() != STATE_BYTES || b[0] > B2S {
            return None;
        }
        let mut w = [0u64; 25];
        for (d, c) in w.iter_mut().zip(b[21..221].as_chunks::<8>().0) {
            *d = u64::from_le_bytes(*c);
        }
        let mut len = [0u8; 16];
        len.copy_from_slice(&b[5..21]);
        let mut buf = [0u8; MAX_BLOCK];
        buf.copy_from_slice(&b[221..]);
        // Buffered bytes: fewer than a block, or a whole one that BLAKE2
        // keeps until it knows whether it is the last. A guest can pass
        // any `bytes` as a state.
        let n = usize::from(u16::from_le_bytes([b[3], b[4]]));
        if n > block(b[0]) || n == block(b[0]) && b[0] < B2B {
            return None;
        }
        Some(St {
            alg: b[0],
            out: b[1],
            last: b[2],
            n,
            len: u128::from_le_bytes(len),
            w,
            buf,
        })
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn store(&self, b: &mut [u8]) {
        if b.len() != STATE_BYTES {
            return;
        }
        b[0] = self.alg;
        b[1] = self.out;
        b[2] = self.last;
        b[3..5].copy_from_slice(&(self.n as u16).to_le_bytes());
        b[5..21].copy_from_slice(&self.len.to_le_bytes());
        for (c, w) in b[21..221].as_chunks_mut::<8>().0.iter_mut().zip(self.w) {
            c.copy_from_slice(&w.to_le_bytes());
        }
        b[221..].copy_from_slice(&self.buf);
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn compress(&mut self, fin: bool) {
        let bs = block(self.alg);
        let blk = &self.buf[..bs];
        match self.alg {
            MD5 => md5(&mut self.w, blk),
            SHA1 => sha1(&mut self.w, blk),
            SHA224 | SHA256 => sha256(&mut self.w, blk),
            SHA384 | SHA512 => sha512(&mut self.w, blk),
            B2B => blake2b(&mut self.w, blk, self.len, fin, fin && self.last != 0),
            B2S => blake2s(
                &mut self.w,
                blk,
                self.len as u64,
                fin,
                fin && self.last != 0,
            ),
            _ => {
                for (l, c) in self.w.iter_mut().zip(blk.as_chunks::<8>().0) {
                    *l ^= u64::from_le_bytes(*c);
                }
                keccak(&mut self.w);
            }
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn update(&mut self, mut data: &[u8]) {
        let bs = block(self.alg);
        let blake = self.alg >= B2B;
        while !data.is_empty() {
            if self.n == bs {
                // BLAKE2 keeps the last block until it knows it is last.
                self.len = self.len.wrapping_add(bs as u128);
                self.compress(false);
                self.n = 0;
            }
            let take = (bs - self.n).min(data.len());
            self.buf[self.n..self.n + take].copy_from_slice(&data[..take]);
            self.n += take;
            data = &data[take..];
            if !blake && self.n == bs {
                if !is_keccak(self.alg) {
                    self.len = self.len.wrapping_add(bs as u128);
                }
                self.compress(false);
                self.n = 0;
            }
        }
    }

    /// Pads a Keccak state's last block and absorbs it: its words are then
    /// the first block of output.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn keccak_pad(&mut self) {
        let bs = block(self.alg);
        self.buf[self.n..].fill(0);
        self.buf[self.n] ^= if self.alg >= SHAKE128 { 0x1f } else { 0x06 };
        self.buf[bs - 1] ^= 0x80;
        self.compress(false);
    }

    /// The digest (`n` bytes for SHAKE), from a copy of the state.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn digest(mut self, n: usize) -> Vec<u8> {
        let alg = self.alg;
        let bs = block(alg);
        let mut out = Vec::new();
        if alg >= B2B {
            self.len = self.len.wrapping_add(self.n as u128);
            self.buf[self.n..].fill(0);
            self.compress(true);
            for w in &self.w[..8] {
                if alg == B2B {
                    out.extend_from_slice(&w.to_le_bytes());
                } else {
                    out.extend_from_slice(&(*w as u32).to_le_bytes());
                }
            }
            out.truncate(self.out as usize);
        } else if is_keccak(alg) {
            self.keccak_pad();
            let want = if alg >= SHAKE128 {
                n
            } else {
                self.out as usize
            };
            loop {
                for l in &self.w[..bs / 8] {
                    out.extend_from_slice(&l.to_le_bytes());
                }
                if out.len() >= want {
                    break;
                }
                keccak(&mut self.w);
            }
            out.truncate(want);
        } else {
            let bits = self.len.wrapping_add(self.n as u128).wrapping_mul(8);
            let lb = if bs == 128 { 16 } else { 8 };
            let mut pad = alloc::vec![0x80u8];
            while (self.n + pad.len()) % bs != bs - lb {
                pad.push(0);
            }
            if alg == MD5 {
                pad.extend_from_slice(&(bits as u64).to_le_bytes());
            } else if lb == 16 {
                pad.extend_from_slice(&bits.to_be_bytes());
            } else {
                pad.extend_from_slice(&(bits as u64).to_be_bytes());
            }
            self.update(&pad);
            for w in &self.w[..8] {
                match alg {
                    MD5 => out.extend_from_slice(&(*w as u32).to_le_bytes()),
                    SHA384 | SHA512 => out.extend_from_slice(&w.to_be_bytes()),
                    _ => out.extend_from_slice(&(*w as u32).to_be_bytes()),
                }
            }
            out.truncate(self.out as usize);
        }
        out
    }
}

fn md5(st: &mut [u64; 25], blk: &[u8]) {
    let mut m = [0u32; 16];
    for (d, c) in m.iter_mut().zip(blk.as_chunks::<4>().0) {
        *d = u32::from_le_bytes(*c);
    }
    let h: [u32; 4] = core::array::from_fn(|i| st[i] as u32);
    let [mut a, mut b, mut c, mut d] = h;
    for i in 0..64 {
        let (f, g) = match i / 16 {
            0 => ((b & c) | (!b & d), i),
            1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
            2 => (b ^ c ^ d, (3 * i + 5) % 16),
            _ => (c ^ (b | !d), (7 * i) % 16),
        };
        let t = d;
        d = c;
        c = b;
        b = b.wrapping_add(
            a.wrapping_add(f)
                .wrapping_add(MD5_K[i])
                .wrapping_add(m[g])
                .rotate_left(MD5_S[(i / 16) * 4 + i % 4]),
        );
        a = t;
    }
    for (s, v) in st.iter_mut().zip([a, b, c, d].iter().zip(h)) {
        *s = u64::from(v.0.wrapping_add(v.1));
    }
}

fn sha1(st: &mut [u64; 25], blk: &[u8]) {
    let mut w = [0u32; 80];
    for (d, c) in w.iter_mut().zip(blk.as_chunks::<4>().0) {
        *d = u32::from_be_bytes(*c);
    }
    for i in 16..80 {
        w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
    }
    let h: [u32; 5] = core::array::from_fn(|i| st[i] as u32);
    let [mut a, mut b, mut c, mut d, mut e] = h;
    for (i, wi) in w.iter().enumerate() {
        let (f, k) = match i / 20 {
            0 => ((b & c) | (!b & d), 0x5a82_7999),
            1 => (b ^ c ^ d, 0x6ed9_eba1),
            2 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
            _ => (b ^ c ^ d, 0xca62_c1d6),
        };
        let t = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(*wi);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = t;
    }
    for (s, v) in st.iter_mut().zip([a, b, c, d, e].iter().zip(h)) {
        *s = u64::from(v.0.wrapping_add(v.1));
    }
}

fn sha256(st: &mut [u64; 25], blk: &[u8]) {
    let mut w = [0u32; 64];
    for (d, c) in w.iter_mut().zip(blk.as_chunks::<4>().0) {
        *d = u32::from_be_bytes(*c);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let h: [u32; 8] = core::array::from_fn(|i| st[i] as u32);
    let mut v = h;
    for i in 0..64 {
        let [a, b, c, d, e, f, g, hh] = v;
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let t1 = hh
            .wrapping_add(s1)
            .wrapping_add((e & f) ^ (!e & g))
            .wrapping_add(K256[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let t2 = s0.wrapping_add((a & b) ^ (a & c) ^ (b & c));
        v = [t1.wrapping_add(t2), a, b, c, d.wrapping_add(t1), e, f, g];
    }
    for (s, x) in st.iter_mut().zip(v.iter().zip(h)) {
        *s = u64::from(x.0.wrapping_add(x.1));
    }
}

fn sha512(st: &mut [u64; 25], blk: &[u8]) {
    let mut w = [0u64; 80];
    for (d, c) in w.iter_mut().zip(blk.as_chunks::<8>().0) {
        *d = u64::from_be_bytes(*c);
    }
    for i in 16..80 {
        let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
        let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let mut v: [u64; 8] = core::array::from_fn(|i| st[i]);
    for i in 0..80 {
        let [a, b, c, d, e, f, g, hh] = v;
        let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
        let t1 = hh
            .wrapping_add(s1)
            .wrapping_add((e & f) ^ (!e & g))
            .wrapping_add(K512[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
        let t2 = s0.wrapping_add((a & b) ^ (a & c) ^ (b & c));
        v = [t1.wrapping_add(t2), a, b, c, d.wrapping_add(t1), e, f, g];
    }
    for (s, x) in st.iter_mut().zip(v) {
        *s = s.wrapping_add(x);
    }
}

fn keccak(a: &mut [u64; 25]) {
    for rc in KECCAK_RC {
        let mut c = [0u64; 5];
        for x in 0..5 {
            c[x] = a[x] ^ a[x + 5] ^ a[x + 10] ^ a[x + 15] ^ a[x + 20];
        }
        for x in 0..5 {
            let d = c[(x + 4) % 5] ^ c[(x + 1) % 5].rotate_left(1);
            for y in 0..5 {
                a[y * 5 + x] ^= d;
            }
        }
        let mut t = a[1];
        for i in 0..24 {
            let j = KECCAK_PI[i];
            let tmp = a[j];
            a[j] = t.rotate_left(KECCAK_ROT[i]);
            t = tmp;
        }
        for y in 0..5 {
            let r: [u64; 5] = core::array::from_fn(|x| a[y * 5 + x]);
            for x in 0..5 {
                a[y * 5 + x] = r[x] ^ (!r[(x + 1) % 5] & r[(x + 2) % 5]);
            }
        }
        a[0] ^= rc;
    }
}

/// BLAKE2's mixing rounds over `v` with message `m` (generic over the
/// word, with the word's four rotation counts).
macro_rules! blake2_rounds {
    ($v:ident, $m:ident, $rounds:expr, $r:expr) => {
        for r in 0..$rounds {
            let s = &SIGMA[r % 10];
            for (i, (a, b, c, d)) in [
                (0, 4, 8, 12),
                (1, 5, 9, 13),
                (2, 6, 10, 14),
                (3, 7, 11, 15),
                (0, 5, 10, 15),
                (1, 6, 11, 12),
                (2, 7, 8, 13),
                (3, 4, 9, 14),
            ]
            .into_iter()
            .enumerate()
            {
                $v[a] = $v[a].wrapping_add($v[b]).wrapping_add($m[s[2 * i]]);
                $v[d] = ($v[d] ^ $v[a]).rotate_right($r[0]);
                $v[c] = $v[c].wrapping_add($v[d]);
                $v[b] = ($v[b] ^ $v[c]).rotate_right($r[1]);
                $v[a] = $v[a].wrapping_add($v[b]).wrapping_add($m[s[2 * i + 1]]);
                $v[d] = ($v[d] ^ $v[a]).rotate_right($r[2]);
                $v[c] = $v[c].wrapping_add($v[d]);
                $v[b] = ($v[b] ^ $v[c]).rotate_right($r[3]);
            }
        }
    };
}

fn blake2b(h: &mut [u64; 25], blk: &[u8], t: u128, fin: bool, last: bool) {
    let mut m = [0u64; 16];
    for (d, c) in m.iter_mut().zip(blk.as_chunks::<8>().0) {
        *d = u64::from_le_bytes(*c);
    }
    let mut v = [0u64; 16];
    v[..8].copy_from_slice(&h[..8]);
    v[8..].copy_from_slice(&IV512);
    v[12] ^= t as u64;
    v[13] ^= (t >> 64) as u64;
    if fin {
        v[14] = !v[14];
    }
    if last {
        v[15] = !v[15];
    }
    blake2_rounds!(v, m, 12, [32, 24, 16, 63]);
    for i in 0..8 {
        h[i] ^= v[i] ^ v[i + 8];
    }
}

fn blake2s(h: &mut [u64; 25], blk: &[u8], t: u64, fin: bool, last: bool) {
    let mut m = [0u32; 16];
    for (d, c) in m.iter_mut().zip(blk.as_chunks::<4>().0) {
        *d = u32::from_le_bytes(*c);
    }
    let mut v = [0u32; 16];
    for i in 0..8 {
        v[i] = h[i] as u32;
    }
    v[8..].copy_from_slice(&IV256);
    v[12] ^= t as u32;
    v[13] ^= (t >> 32) as u32;
    if fin {
        v[14] = !v[14];
    }
    if last {
        v[15] = !v[15];
    }
    blake2_rounds!(v, m, 10, [16, 12, 8, 7]);
    for i in 0..8 {
        h[i] = u64::from(h[i] as u32 ^ v[i] ^ v[i + 8]);
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn bad_state(vm: &mut Vm) -> Raised {
    vm.err(T::TypeError, "hashlib: bad hash state")
}

/// The state in `v` (a `bytes` from `_h_new`).
#[cfg_attr(mm_shipped, optimize(size))]
fn load(vm: &mut Vm, v: Value) -> Result<St, Raised> {
    if !Vm::is_bytes(v) {
        return Err(bad_state(vm));
    }
    match St::load(vm.bytes_data(v)) {
        Some(s) => Ok(s),
        None => Err(bad_state(vm)),
    }
}

/// Writes `st` back into the state object `v`.
#[cfg_attr(mm_shipped, optimize(size))]
fn store(vm: &mut Vm, v: Value, st: &St) {
    if let Some(b) = vm.vslot_o(v, 0).as_byte_ref()
        && let Ok(data) = vm.heap.bytes_mut(b)
    {
        st.store(data);
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn new_state(vm: &mut Vm, st: &St) -> Result<Value, Raised> {
    let mut b = alloc::vec![0u8; STATE_BYTES];
    st.store(&mut b);
    vm.new_bytes(&b)
}

/// `_h_new(alg, params=None, last_node=False)`: a fresh state; for BLAKE2,
/// `params` is the parameter block (`hashlib.py` built and checked it;
/// its first byte is the digest size) and the key block, if any, is fed
/// by the caller.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn h_new(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let alg = vm.int_arg(vm.arg_o(&a, 0), "")?;
    let Ok(alg) = u8::try_from(alg) else {
        return Err(bad_state(vm));
    };
    if alg > B2S {
        return Err(bad_state(vm));
    }
    let mut st = St::new(alg);
    let p = vm.arg_o(&a, 1);
    if alg >= B2B && Vm::is_bytes(p) {
        let p = vm.bytes_data(p);
        if p.len() != block(alg) / 2 || p[0] == 0 {
            return Err(bad_state(vm));
        }
        st.out = p[0];
        if alg == B2B {
            for (w, c) in st.w.iter_mut().zip(p.as_chunks::<8>().0) {
                *w ^= u64::from_le_bytes(*c);
            }
        } else {
            for (w, c) in st.w.iter_mut().zip(p.as_chunks::<4>().0) {
                *w ^= u64::from(u32::from_le_bytes(*c));
            }
        }
    } else if alg >= B2B {
        // The default parameter block: no key, fanout 1, depth 1.
        st.w[0] ^= 0x0101_0000 | u64::from(st.out);
    }
    st.last = u8::from(vm.arg_o(&a, 2) == Value::TRUE);
    new_state(vm, &st)
}

/// `_h_update(state, data)`: feeds `data` (bytes).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn h_update(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let sv = vm.arg_o(&a, 0);
    // A bad state fails before any work.
    load(vm, sv)?;
    let dv = vm.arg_o(&a, 1);
    if !Vm::is_bytes(dv) {
        return Err(vm.err(T::TypeError, "object supporting the buffer API required"));
    }
    let n = vm.bytes_data(dv).len();
    let mut j = Job::new(job::HASH, dv, 0, n, 0);
    j.v[1] = sv;
    vm.job_run(j)
}

/// Feeds `data[i..i + n]` (bytes) to the state in `sv` (a step of
/// `_h_update`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn feed(vm: &mut Vm, sv: Value, dv: Value, i: usize, n: usize) -> Result<(), Raised> {
    let mut st = load(vm, sv)?;
    st.update(vm.bytes_data(dv).get(i..i + n).unwrap_or(&[]));
    store(vm, sv, &st);
    Ok(())
}

/// `_h_copy(state)`: an independent copy.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn h_copy(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let st = load(vm, vm.arg_o(&a, 0))?;
    new_state(vm, &st)
}

/// `_h_digest(state, length)`: the digest (`length` bytes for SHAKE).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn h_digest(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let st = load(vm, vm.arg_o(&a, 0))?;
    let n = vm.int_arg(vm.arg_o(&a, 1), "")?;
    let Ok(n) = usize::try_from(n) else {
        return Err(vm.err(T::ValueError, "negative digest length"));
    };
    if st.alg >= SHAKE128 && is_keccak(st.alg) {
        // Any length: squeezed into the result a step at a time.
        vm.burn(8);
        let mut sq = st;
        sq.keccak_pad();
        let sv = new_state(vm, &sq)?;
        let mut j = vm.job_fixed(job::SQUEEZE, Value::NONE, 0, n, R_BYTES, n)?;
        j.v[1] = sv;
        return vm.job_run(j);
    }
    vm.burn(8);
    let d = st.digest(n);
    vm.new_bytes(&d)
}

impl Vm {
    /// A step of a SHAKE digest ([`job::SQUEEZE`]): the state in value slot
    /// 1 is ready to give its next block of output.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn job_squeeze(&mut self, j: &mut Job, start: u64) -> Result<Option<Value>, Raised> {
        let mut st = load(self, j.v[1])?;
        let Some(out) = j.v[2].as_byte_ref() else {
            return Err(self.bad_step());
        };
        let (n, bs) = (j.w[W_END] as usize, block(st.alg));
        let mut o = j.w[W_O] as usize;
        let mut buf = [0u8; MAX_BLOCK];
        while o < n {
            if self.job_left(start) == 0 {
                store(self, j.v[1], &st);
                j.w[W_O] = o as u64;
                return Ok(None);
            }
            for (c, l) in buf.as_chunks_mut::<8>().0.iter_mut().zip(&st.w) {
                *c = l.to_le_bytes();
            }
            let k = bs.min(n - o);
            let ok = self
                .heap
                .bytes_mut(out)
                .ok()
                .and_then(|d| d.get_mut(o..o + k))
                .map(|d| d.copy_from_slice(&buf[..k]))
                .is_some();
            if !ok {
                return Err(self.bad_step());
            }
            o += k;
            self.burn(bs as u64 / 16);
            if o < n {
                keccak(&mut st.w);
            }
        }
        self.job_wrap(j, out, 0).map(Some)
    }
}

/// `_h_pbkdf2(inner, outer, u, acc, n)`: `n` PBKDF2 rounds, where `inner`
/// and `outer` are the HMAC states keyed with the password; returns the
/// new `(u, acc)`. `hashlib.py` calls it in slices, so a long run parks.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn h_pbkdf2(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let inner = load(vm, vm.arg_o(&a, 0))?;
    let outer = load(vm, vm.arg_o(&a, 1))?;
    // Digests, as `hashlib.py` passes them (anything longer is cut, not
    // copied).
    let (mut u, mut acc) = ([0u8; 64], [0u8; 64]);
    let take = |d: &[u8], to: &mut [u8; 64]| {
        let k = d.len().min(64);
        to[..k].copy_from_slice(&d[..k]);
        k
    };
    let mut ul = take(vm.bytes_data(vm.arg_o(&a, 2)), &mut u);
    let al = take(vm.bytes_data(vm.arg_o(&a, 3)), &mut acc);
    let n = vm.int_arg(vm.arg_o(&a, 4), "")?.clamp(0, 1 << 16);
    let ilen = inner.out as usize;
    for _ in 0..n {
        let mut s = inner.clone();
        s.update(&u[..ul]);
        let d = s.digest(ilen);
        let mut s = outer.clone();
        s.update(&d);
        ul = take(&s.digest(ilen), &mut u);
        for (x, y) in acc[..al].iter_mut().zip(&u[..ul]) {
            *x ^= y;
        }
    }
    vm.burn(n as u64 * (block(inner.alg) as u64 / 4));
    let uv = vm.new_bytes(&u[..ul])?;
    let av = vm.new_bytes(&acc[..al])?;
    vm.alloc_tuple(&[uv, av])
}
