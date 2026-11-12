//! A small LZ77 + Huffman codec (deflate's length and distance codes, a
//! simpler header) for code embedded in the binary: build scripts [`pack`]
//! encoded programs and the engine [`unpack`]s one when it is first needed.
//!
//! Layout: the unpacked length (LEB128), the code lengths of the 285
//! literal/length symbols and the 30 distance symbols (one nibble each),
//! then the symbols as a bit stream (LSB first; Huffman codes MSB first,
//! canonical as in deflate). There is no end symbol: unpacking stops at the
//! length. Hand-written so the engine takes no dependency (docs/spec.md §5).

use alloc::{vec, vec::Vec};

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// Literal bytes, then one symbol per length code.
const NLIT: usize = 256 + 29;
const NDIST: usize = 30;
const MAX_BITS: usize = 15;
/// [`MAX_BITS`] as a bit count.
const LONGEST: u32 = 15;
const _: () = assert!(LONGEST as usize == MAX_BITS);
const HEADER: usize = (NLIT + NDIST).div_ceil(2);
/// The largest unpacked size [`unpack`] accepts.
const MAX_LEN: usize = 1 << 24;

// ------------------------------------------------------------- unpacking

/// Bits a [`Huff`] table resolves in one lookup.
const FAST: u32 = 10;

/// A canonical Huffman decoder: a lookup table for codes up to [`FAST`]
/// bits, and codes per length and symbols by code for longer ones.
struct Huff {
    /// By the next `FAST` stream bits: `symbol << 4 | length`, or 0.
    table: Vec<u16>,
    count: [u16; MAX_BITS + 1],
    /// By length: the first canonical code, and the index in `sym` of
    /// its symbol.
    first: [u16; MAX_BITS + 1],
    index: [u16; MAX_BITS + 1],
    sym: Vec<u16>,
}

impl Huff {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "code lengths are at most 15"
    )]
    fn new(lens: &[u8]) -> Huff {
        let mut count = [0u16; MAX_BITS + 1];
        for &l in lens {
            count[usize::from(l) & MAX_BITS] += 1;
        }
        count[0] = 0;
        let mut offs = [0u16; MAX_BITS + 2];
        let (mut first, mut index) = ([0u16; MAX_BITS + 1], [0u16; MAX_BITS + 1]);
        let mut code = 0u16;
        for l in 1..=MAX_BITS {
            offs[l + 1] = offs[l].wrapping_add(count[l]);
            first[l] = code;
            index[l] = offs[l];
            code = code.wrapping_add(count[l]) << 1;
        }
        let mut sym = vec![0u16; usize::from(offs[MAX_BITS + 1])];
        for (s, &l) in lens.iter().enumerate() {
            let l = usize::from(l) & MAX_BITS;
            if l != 0 {
                if let Some(slot) = sym.get_mut(usize::from(offs[l])) {
                    *slot = u16::try_from(s).unwrap_or(0);
                }
                offs[l] += 1;
            }
        }
        // Canonical order (`sym`) by length: at each length the table so
        // far is doubled, so shorter codes repeat, then this length's codes
        // are set. Slots left 0 start longer codes.
        let mut table = Vec::with_capacity(1 << FAST);
        table.push(0u16);
        for l in 1..=FAST as usize {
            table.extend_from_within(..);
            let start = usize::from(index[l]);
            let run = sym.get(start..start + usize::from(count[l])).unwrap_or(&[]);
            let mut c = first[l];
            for &s in run {
                let rev = c.reverse_bits() >> (16 - l);
                c = c.wrapping_add(1);
                if let Some(slot) = table.get_mut(usize::from(rev)) {
                    *slot = s << 4 | l as u16;
                }
            }
        }
        Huff {
            table,
            count,
            first,
            index,
            sym,
        }
    }

    /// The symbol `buf` (stream bits, LSB first; at least 15) starts
    /// with, and its code length.
    #[allow(
        clippy::inline_always,
        reason = "mm-compile builds at opt-level s, which leaves it a call per symbol [M]"
    )]
    #[inline(always)]
    fn decode(&self, buf: u64) -> Option<(usize, u32)> {
        let e = *self
            .table
            .get(usize::try_from(buf & ((1 << FAST) - 1)).ok()?)?;
        if e != 0 {
            return Some((usize::from(e >> 4), u32::from(e & 15)));
        }
        // A longer code: canonical codes of length `len` are `first[len]`
        // and the next `count[len] - 1` values, MSB first in the stream.
        let rev = ((buf & 0x7fff) as u32).reverse_bits() >> 17;
        for len in FAST + 1..=LONGEST {
            let code = rev >> (LONGEST - len);
            let (first, count) = (
                u32::from(*self.first.get(len as usize)?),
                u32::from(*self.count.get(len as usize)?),
            );
            if code.wrapping_sub(first) < count {
                let i = u32::from(*self.index.get(len as usize)?) + code - first;
                let s = self.sym.get(usize::try_from(i).ok()?)?;
                return Some((usize::from(*s), len));
            }
        }
        None
    }
}

/// The bytes [`pack`] packed, or `None` if `src` is malformed.
#[must_use]
pub fn unpack(src: &[u8]) -> Option<Vec<u8>> {
    let mut budget = u64::MAX;
    unpack_bounded(src, &mut budget).ok()
}

/// Like [`unpack`], admitting the output and Huffman workspace before
/// allocating. On success deducts both from `budget`; failure leaves it
/// unchanged. The workspace charge bounds both tables, symbol/code buffers
/// and unpacked header, even though their allocations do not all coexist.
pub fn unpack_bounded(src: &[u8], budget: &mut u64) -> Result<Vec<u8>, crate::DecodeError> {
    let mut remaining = Some(*budget);
    let bytes = unpack_in(src, &mut remaining).ok_or(if remaining.is_none() {
        crate::DecodeError::TooLarge
    } else {
        crate::DecodeError::Invalid
    })?;
    *budget = remaining.unwrap_or(0);
    Ok(bytes)
}

/// Like [`unpack_bounded`], for one part [`pack_shared`] packed: `header`
/// is the shared header, `src` the part.
pub fn unpack_shared_bounded(
    header: &[u8],
    src: &[u8],
    budget: &mut u64,
) -> Result<Vec<u8>, crate::DecodeError> {
    let mut remaining = Some(*budget);
    let bytes = (|| {
        let (n, i) = unpacked_len(src)?;
        decode(n, header, src.get(i..)?, &mut remaining)
    })()
    .ok_or(if remaining.is_none() {
        crate::DecodeError::TooLarge
    } else {
        crate::DecodeError::Invalid
    })?;
    *budget = remaining.unwrap_or(0);
    Ok(bytes)
}

/// The unpacked length `src` starts with, and the bytes it takes.
fn unpacked_len(src: &[u8]) -> Option<(usize, usize)> {
    let mut n = 0usize;
    let mut i = 0;
    loop {
        let b = *src.get(i)?;
        n |= usize::from(b & 0x7f) << (7 * i);
        i += 1;
        if b & 0x80 == 0 {
            break;
        }
        if i == 4 {
            return None;
        }
    }
    (n <= MAX_LEN).then_some((n, i))
}

fn unpack_in(src: &[u8], budget: &mut Option<u64>) -> Option<Vec<u8>> {
    let (n, i) = unpacked_len(src)?;
    let (header, body) = src.get(i..)?.split_at_checked(HEADER)?;
    decode(n, header, body, budget)
}

/// The `n` bytes `body` codes with the code lengths in `header`.
fn decode(n: usize, header: &[u8], body: &[u8], budget: &mut Option<u64>) -> Option<Vec<u8>> {
    if header.len() != HEADER {
        return None;
    }
    *budget = (*budget)?.checked_sub((n + scratch()) as u64);
    (*budget)?;
    let (lit, dist) = tables(header)?;
    decode_with(n, &lit, &dist, body)
}

/// The literal/length and distance decoders of `header`.
fn tables(header: &[u8]) -> Option<(Huff, Huff)> {
    let lens: Vec<u8> = header.iter().flat_map(|&b| [b & 15, b >> 4]).collect();
    Some((
        Huff::new(lens.get(..NLIT)?),
        Huff::new(lens.get(NLIT..NLIT + NDIST)?),
    ))
}

/// Unpacks several parts [`pack_shared`] packed against one header,
/// building the header's decoders once. Each part is charged as
/// [`unpack_shared_bounded`] charges it.
pub struct Unpacker<'a> {
    header: &'a [u8],
    tables: Option<(Huff, Huff)>,
}

impl<'a> Unpacker<'a> {
    /// An unpacker for parts packed against `header`; its decoders are
    /// built on the first part.
    #[must_use]
    pub fn new(header: &'a [u8]) -> Unpacker<'a> {
        Unpacker {
            header,
            tables: None,
        }
    }

    /// [`unpack_shared_bounded`] of `src` against this header.
    pub fn unpack_bounded(
        &mut self,
        src: &[u8],
        budget: &mut u64,
    ) -> Result<Vec<u8>, crate::DecodeError> {
        if self.header.len() != HEADER {
            return Err(crate::DecodeError::Invalid);
        }
        let (n, i) = unpacked_len(src).ok_or(crate::DecodeError::Invalid)?;
        let need = (n + scratch()) as u64;
        let left = budget
            .checked_sub(need)
            .ok_or(crate::DecodeError::TooLarge)?;
        if self.tables.is_none() {
            self.tables = tables(self.header);
        }
        let (lit, dist) = self.tables.as_ref().ok_or(crate::DecodeError::Invalid)?;
        let out = src
            .get(i..)
            .and_then(|body| decode_with(n, lit, dist, body))
            .ok_or(crate::DecodeError::Invalid)?;
        *budget = left;
        Ok(out)
    }
}

/// The workspace one decode charges beside its output ([`unpack_bounded`],
/// [`unpack_shared_bounded`]), held only while it runs.
#[must_use]
pub fn workspace() -> u64 {
    scratch() as u64
}

/// The workspace one decode charges beside its output: both tables,
/// symbol/code buffers and the unpacked header.
fn scratch() -> usize {
    2 * (1 << FAST) * size_of::<u16>()
        + (NLIT + NDIST) * (size_of::<u16>() + size_of::<u32>())
        + HEADER * 2
}

/// The `n` bytes `body` codes with these decoders.
fn decode_with(n: usize, lit: &Huff, dist: &Huff, body: &[u8]) -> Option<Vec<u8>> {
    // Stream bits, LSB first: `buf` holds `nb` of them, `at` is the next
    // byte (past the end reads zeros; unpacking stops at the length).
    let (mut buf, mut nb, mut at) = (0u64, 0u32, 0usize);
    let mut out = vec![0u8; n];
    let mut pos = 0;
    while pos < n {
        // One token takes at most 15 + 5 + 15 + 13 bits.
        if let Some(w) = body
            .get(at..at + 8)
            .and_then(|c| <[u8; 8]>::try_from(c).ok())
        {
            buf |= u64::from_le_bytes(w) << nb;
            let k = (63 - nb) >> 3;
            at += k as usize;
            nb += k * 8;
        } else {
            while nb <= 56 {
                buf |= u64::from(body.get(at).copied().unwrap_or(0)) << nb;
                at += 1;
                nb += 8;
            }
        }
        let (s, l) = lit.decode(buf)?;
        buf >>= l;
        nb -= l;
        if s < 256 {
            *out.get_mut(pos)? = u8::try_from(s).ok()?;
            pos += 1;
            continue;
        }
        let k = s - 256;
        let e = u32::from(*LEN_EXTRA.get(k)?);
        let len = usize::from(*LEN_BASE.get(k)?) + usize::try_from(buf & ((1 << e) - 1)).ok()?;
        buf >>= e;
        nb -= e;
        let (d, l) = dist.decode(buf)?;
        buf >>= l;
        nb -= l;
        let e = u32::from(*DIST_EXTRA.get(d)?);
        let back = usize::from(*DIST_BASE.get(d)?) + usize::try_from(buf & ((1 << e) - 1)).ok()?;
        buf >>= e;
        nb -= e;
        if back > pos || pos + len > n {
            return None;
        }
        if back >= len {
            out.copy_within(pos - back..pos - back + len, pos);
        } else {
            for j in pos..pos + len {
                *out.get_mut(j)? = *out.get(j - back)?;
            }
        }
        pos += len;
    }
    Some(out)
}

// --------------------------------------------------------------- packing

struct BitOut {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl BitOut {
    fn put(&mut self, v: u32, bits: u32) {
        for i in 0..bits {
            self.acc |= ((v >> i) & 1) << self.n;
            self.n += 1;
            if self.n == 8 {
                self.out.push(u8::try_from(self.acc).unwrap_or(0));
                self.acc = 0;
                self.n = 0;
            }
        }
    }

    fn code(&mut self, code: u32, len: u8) {
        for i in (0..u32::from(len)).rev() {
            self.put(code >> i, 1);
        }
    }
}

enum Token {
    Lit(u8),
    Match(usize, usize),
}

/// The code index for `v` in `base` (the last base not above it).
fn code_of(base: &[u16], v: usize) -> usize {
    base.iter()
        .rposition(|&b| usize::from(b) <= v)
        .unwrap_or_default()
}

/// Huffman code lengths for `freq`, at most [`MAX_BITS`].
fn lengths(freq: &[u64]) -> Vec<u8> {
    let mut freq = freq.to_vec();
    loop {
        let leaves: Vec<usize> = (0..freq.len()).filter(|&s| freq[s] > 0).collect();
        let mut lens = vec![0u8; freq.len()];
        if leaves.len() == 1 {
            lens[leaves[0]] = 1;
        }
        if leaves.len() <= 1 {
            return lens;
        }
        // Nodes: leaves first, then merges; `parent` links up.
        let mut weight: Vec<u64> = leaves.iter().map(|&s| freq[s]).collect();
        let mut parent = vec![usize::MAX; leaves.len()];
        let mut live: Vec<usize> = (0..leaves.len()).collect();
        while live.len() > 1 {
            live.sort_by_key(|&i| core::cmp::Reverse((weight[i], i)));
            let (Some(a), Some(b)) = (live.pop(), live.pop()) else {
                break;
            };
            let p = weight.len();
            weight.push(weight[a] + weight[b]);
            parent.push(usize::MAX);
            parent[a] = p;
            parent[b] = p;
            live.push(p);
        }
        let mut longest = 0;
        for (k, &s) in leaves.iter().enumerate() {
            let mut depth = 0;
            let mut i = k;
            while parent[i] != usize::MAX {
                i = parent[i];
                depth += 1;
            }
            longest = longest.max(depth);
            lens[s] = u8::try_from(depth).unwrap_or(u8::MAX);
        }
        if longest <= MAX_BITS {
            return lens;
        }
        for f in &mut freq {
            if *f > 0 {
                *f = (*f / 2).max(1);
            }
        }
    }
}

/// Canonical codes for `lens` (deflate's assignment).
fn codes(lens: &[u8]) -> Vec<u32> {
    let mut count = [0u32; MAX_BITS + 1];
    for &l in lens {
        count[usize::from(l)] += 1;
    }
    count[0] = 0;
    let mut next = [0u32; MAX_BITS + 1];
    let mut code = 0;
    for bits in 1..=MAX_BITS {
        code = (code + count[bits - 1]) << 1;
        next[bits] = code;
    }
    lens.iter()
        .map(|&l| {
            let l = usize::from(l);
            if l == 0 {
                return 0;
            }
            let c = next[l];
            next[l] += 1;
            c
        })
        .collect()
}

/// The LZ77 tokens of `data`: hash chains over 3-byte prefixes, a 32 KiB
/// window, one step of lazy matching.
fn tokens(data: &[u8]) -> Vec<Token> {
    const WINDOW: usize = 32768;
    const CHAIN: usize = 1024;
    let hash = |i: usize| -> usize {
        (usize::from(data[i]) << 10 ^ usize::from(data[i + 1]) << 5 ^ usize::from(data[i + 2]))
            & 0x7fff
    };
    let mut head = vec![usize::MAX; 0x8000];
    let mut prev = vec![usize::MAX; data.len()];
    let longest = |i: usize, head: &[usize], prev: &[usize]| -> (usize, usize) {
        let mut best = (0, 0);
        if i + 3 > data.len() {
            return best;
        }
        let max = (data.len() - i).min(258);
        let mut cand = head[hash(i)];
        let mut steps = 0;
        while cand != usize::MAX && i - cand <= WINDOW && steps < CHAIN {
            let n = data[cand..]
                .iter()
                .zip(&data[i..i + max])
                .take_while(|(a, b)| a == b)
                .count();
            if n > best.0 {
                best = (n, i - cand);
                if n == max {
                    break;
                }
            }
            cand = prev[cand];
            steps += 1;
        }
        best
    };
    let insert = |i: usize, head: &mut [usize], prev: &mut [usize]| {
        if i + 3 <= data.len() {
            let h = hash(i);
            prev[i] = head[h];
            head[h] = i;
        }
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let (len, back) = longest(i, &head, &prev);
        if len >= 3 {
            insert(i, &mut head, &mut prev);
            let (next, _) = longest(i + 1, &head, &prev);
            if next > len {
                out.push(Token::Lit(data[i]));
                i += 1;
                continue;
            }
            out.push(Token::Match(len, back));
            for j in i + 1..i + len {
                insert(j, &mut head, &mut prev);
            }
            i += len;
        } else {
            insert(i, &mut head, &mut prev);
            out.push(Token::Lit(data[i]));
            i += 1;
        }
    }
    out
}

/// Code lengths for the symbols of `toks`.
fn token_lengths<'a>(toks: impl IntoIterator<Item = &'a Token>) -> (Vec<u8>, Vec<u8>) {
    let mut lit_freq = vec![0u64; NLIT];
    let mut dist_freq = vec![0u64; NDIST];
    for t in toks {
        match *t {
            Token::Lit(b) => lit_freq[usize::from(b)] += 1,
            Token::Match(len, back) => {
                lit_freq[256 + code_of(&LEN_BASE, len)] += 1;
                dist_freq[code_of(&DIST_BASE, back)] += 1;
            }
        }
    }
    (lengths(&lit_freq), lengths(&dist_freq))
}

/// Per position, the matches the window offers: `(n, back)` pairs by
/// growing `n`, `back` the nearest source for every length up to `n`.
fn matches(data: &[u8]) -> Vec<Vec<(usize, usize)>> {
    const WINDOW: usize = 32768;
    const CHAIN: usize = 1024;
    let hash = |i: usize| -> usize {
        (usize::from(data[i]) << 10 ^ usize::from(data[i + 1]) << 5 ^ usize::from(data[i + 2]))
            & 0x7fff
    };
    let mut head = vec![usize::MAX; 0x8000];
    let mut prev = vec![usize::MAX; data.len()];
    let mut out = Vec::with_capacity(data.len());
    for i in 0..data.len() {
        let mut here = Vec::new();
        if i + 3 <= data.len() {
            let max = (data.len() - i).min(258);
            let h = hash(i);
            let mut cand = head[h];
            let mut steps = 0;
            let mut best = 2;
            while cand != usize::MAX && i - cand <= WINDOW && steps < CHAIN {
                let n = data[cand..]
                    .iter()
                    .zip(&data[i..i + max])
                    .take_while(|(a, b)| a == b)
                    .count();
                if n > best {
                    best = n;
                    here.push((n, i - cand));
                    if n == max {
                        break;
                    }
                }
                cand = prev[cand];
                steps += 1;
            }
            prev[i] = head[h];
            head[h] = i;
        }
        out.push(here);
    }
    out
}

/// The cheapest tokens for `data` under the given code lengths (a
/// shortest path over positions).
fn optimal(
    data: &[u8],
    found: &[Vec<(usize, usize)>],
    lit_lens: &[u8],
    dist_lens: &[u8],
) -> Vec<Token> {
    let bits = |l: u8| if l == 0 { 16 } else { u64::from(l) };
    let len_cost = |len: usize| {
        let k = code_of(&LEN_BASE, len);
        bits(lit_lens[256 + k]) + u64::from(LEN_EXTRA[k])
    };
    let dist_cost = |back: usize| {
        let d = code_of(&DIST_BASE, back);
        bits(dist_lens[d]) + u64::from(DIST_EXTRA[d])
    };
    let n = data.len();
    let mut cost = vec![0u64; n + 1];
    let mut step = vec![(1usize, 0usize); n];
    for i in (0..n).rev() {
        let mut best = cost[i + 1] + bits(lit_lens[usize::from(data[i])]);
        let mut choice = (1, 0);
        let mut from = 3;
        for &(upto, back) in &found[i] {
            let dc = dist_cost(back);
            for len in from..=upto {
                let c = cost[i + len] + len_cost(len) + dc;
                if c < best {
                    best = c;
                    choice = (len, back);
                }
            }
            from = upto + 1;
        }
        cost[i] = best;
        step[i] = choice;
    }
    let mut toks = Vec::new();
    let mut i = 0;
    while i < n {
        let (len, back) = step[i];
        if back == 0 {
            toks.push(Token::Lit(data[i]));
        } else {
            toks.push(Token::Match(len, back));
        }
        i += len;
    }
    toks
}

/// `data` packed for [`unpack`] (a build-time step; not used by the
/// engine at run time).
#[must_use]
pub fn pack(data: &[u8]) -> Vec<u8> {
    let mut toks = tokens(data);
    let (mut lit_lens, mut dist_lens) = token_lengths(&toks);
    let found = matches(data);
    for _ in 0..3 {
        toks = optimal(data, &found, &lit_lens, &dist_lens);
        (lit_lens, dist_lens) = token_lengths(&toks);
    }
    let mut out = len_prefix(data.len());
    out.extend(header(&lit_lens, &dist_lens));
    emit(out, &toks, &lit_lens, &dist_lens)
}

/// `parts` packed against one shared pair of Huffman codes: the shared
/// header, and each part for [`unpack_shared_bounded`] (its length, then
/// its bits). A build-time step.
#[must_use]
pub fn pack_shared(parts: &[&[u8]]) -> (Vec<u8>, Vec<Vec<u8>>) {
    let found: Vec<_> = parts.iter().map(|d| matches(d)).collect();
    let mut toks: Vec<Vec<Token>> = parts.iter().map(|d| tokens(d)).collect();
    let (mut lit_lens, mut dist_lens) = token_lengths(toks.iter().flatten());
    for _ in 0..3 {
        toks = parts
            .iter()
            .zip(&found)
            .map(|(d, f)| optimal(d, f, &lit_lens, &dist_lens))
            .collect();
        (lit_lens, dist_lens) = token_lengths(toks.iter().flatten());
    }
    let out = parts
        .iter()
        .zip(&toks)
        .map(|(d, t)| emit(len_prefix(d.len()), t, &lit_lens, &dist_lens))
        .collect();
    (header(&lit_lens, &dist_lens), out)
}

fn len_prefix(mut n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let b = u8::try_from(n & 0x7f).unwrap_or(0);
        n >>= 7;
        if n == 0 {
            out.push(b);
            return out;
        }
        out.push(b | 0x80);
    }
}

/// The code lengths, one nibble each.
fn header(lit_lens: &[u8], dist_lens: &[u8]) -> Vec<u8> {
    let all: Vec<u8> = lit_lens.iter().chain(dist_lens).copied().collect();
    all.chunks(2)
        .map(|pair| pair[0] | pair.get(1).copied().unwrap_or(0) << 4)
        .collect()
}

/// `out` followed by the bits of `toks`.
fn emit(out: Vec<u8>, toks: &[Token], lit_lens: &[u8], dist_lens: &[u8]) -> Vec<u8> {
    let (lit_codes, dist_codes) = (codes(lit_lens), codes(dist_lens));
    let mut w = BitOut { out, acc: 0, n: 0 };
    for t in toks {
        match *t {
            Token::Lit(b) => {
                let s = usize::from(b);
                w.code(lit_codes[s], lit_lens[s]);
            }
            Token::Match(len, back) => {
                let k = code_of(&LEN_BASE, len);
                w.code(lit_codes[256 + k], lit_lens[256 + k]);
                w.put(
                    u32::try_from(len - usize::from(LEN_BASE[k])).unwrap_or(0),
                    u32::from(LEN_EXTRA[k]),
                );
                let d = code_of(&DIST_BASE, back);
                w.code(dist_codes[d], dist_lens[d]);
                w.put(
                    u32::try_from(back - usize::from(DIST_BASE[d])).unwrap_or(0),
                    u32::from(DIST_EXTRA[d]),
                );
            }
        }
    }
    if w.n > 0 {
        w.out.push(u8::try_from(w.acc).unwrap_or(0));
    }
    w.out
}

#[cfg(test)]
mod tests {
    use super::{
        FAST, NLIT, Unpacker, pack, pack_shared, unpack, unpack_bounded, unpack_shared_bounded,
    };
    use crate::DecodeError;
    use alloc::vec::Vec;

    #[test]
    fn round_trips() {
        let mut long = Vec::new();
        for i in 0..6_000u32 {
            long.push((i * 7 % 251) as u8);
            if i % 3 == 0 {
                long.extend_from_slice(b"__init__ self");
            }
        }
        for data in [
            &b""[..],
            b"a",
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            b"abcabcabd",
            &long,
        ] {
            let p = pack(data);
            assert_eq!(unpack(&p).as_deref(), Some(data));
            if !data.is_empty() {
                let allowance = data.len() as u64 - 1;
                let mut budget = allowance;
                assert_eq!(unpack_bounded(&p, &mut budget), Err(DecodeError::TooLarge));
                assert_eq!(budget, allowance);
            }
            let allowance = data.len() as u64 + (64 << 10);
            let mut budget = allowance;
            assert_eq!(unpack_bounded(&p, &mut budget).ok().as_deref(), Some(data));
            assert!(allowance - budget >= data.len() as u64);
        }
        assert!(pack(&long).len() < long.len() / 4);
    }

    #[test]
    fn shared_parts_round_trip_alone() {
        let parts: [&[u8]; 4] = [
            b"",
            b"alpha beta alpha beta",
            b"zzzz",
            b"beta gamma \x00\xff beta",
        ];
        let (header, packed) = pack_shared(&parts);
        for (p, data) in packed.iter().zip(parts) {
            let mut budget = u64::MAX;
            assert_eq!(
                unpack_shared_bounded(&header, p, &mut budget).as_deref(),
                Ok(data)
            );
            for cut in 0..p.len() {
                let _ = unpack_shared_bounded(&header, &p[..cut], &mut u64::MAX.clone());
            }
            if !data.is_empty() {
                let mut tight = data.len() as u64 - 1;
                assert_eq!(
                    unpack_shared_bounded(&header, p, &mut tight),
                    Err(DecodeError::TooLarge)
                );
            }
        }
        let mut budget = u64::MAX;
        assert!(unpack_shared_bounded(&header[1..], &packed[1], &mut budget).is_err());
    }

    #[test]
    fn unpacker_matches_unpack_shared() {
        // A skewed, unrepetitive part, so that rare bytes get codes longer
        // than the decoder's lookup table.
        let mut x = 1u32;
        let skewed: Vec<u8> = (0..20_000)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let [_, r, b, _] = x.to_le_bytes();
                if r.is_multiple_of(64) { b } else { r % 6 }
            })
            .collect();
        let parts: [&[u8]; 4] = [b"alpha beta alpha beta", &skewed, b"", b"beta \xff beta"];
        let (header, packed) = pack_shared(&parts);
        let lens: Vec<u8> = header.iter().flat_map(|&b| [b & 15, b >> 4]).collect();
        assert!(lens[..NLIT].iter().any(|&l| u32::from(l) > FAST));
        let mut unpacker = Unpacker::new(&header);
        for (p, data) in packed.iter().zip(parts) {
            let (mut a, mut b) = (1u64 << 30, 1u64 << 30);
            assert_eq!(unpacker.unpack_bounded(p, &mut a).as_deref(), Ok(data));
            assert_eq!(
                unpack_shared_bounded(&header, p, &mut b).as_deref(),
                Ok(data)
            );
            assert_eq!(a, b);
            if !data.is_empty() {
                let mut tight = data.len() as u64 - 1;
                assert_eq!(
                    unpacker.unpack_bounded(p, &mut tight),
                    Err(DecodeError::TooLarge)
                );
                assert_eq!(tight, data.len() as u64 - 1);
            }
            for cut in 0..p.len().min(64) {
                let _ = unpacker.unpack_bounded(&p[..cut], &mut u64::MAX.clone());
            }
        }
        assert!(
            Unpacker::new(&header[1..])
                .unpack_bounded(&packed[0], &mut u64::MAX.clone())
                .is_err()
        );
    }

    #[test]
    fn rejects_malformed_input() {
        let p = pack(b"hello hello hello hello");
        for cut in 0..p.len() {
            let _ = unpack(&p[..cut]);
        }
        assert_eq!(unpack(&[0x80, 0x80, 0x80, 0x80, 0x01]), None);
    }
}
