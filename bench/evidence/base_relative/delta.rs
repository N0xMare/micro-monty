//! The base-relative transform: parse a FULL image, encode it against a
//! base image as changed blocks, and rebuild the exact FULL bytes. Needs only
//! `mm_core::{Reader, xxh64}`, so a size probe can compile it alone.

use mm_core::{Reader, xxh64};

pub type Res<T> = Result<T, String>;

// ---------------------------------------------------------------------------
// The FULL image, parsed (crates/mm-core/src/snapshot.rs, crates/micro-monty
// `snapshot_unguarded`).

/// Session header (magic, format, build id, settings: 53 bytes) plus the
/// heap's config, stats and intern table ref (64 bytes).
pub const PREFIX: usize = 53 + 64;

#[derive(Clone)]
pub struct Seg {
    pub idx: u32,
    pub large: bool,
    pub top: u32,
    pub cap: Option<u32>,
    pub runs: Vec<(u32, u32)>,
    /// `top * 8` bytes; free space zero.
    pub dense: Vec<u8>,
    /// One bit per 8-byte unit: inside a live run.
    pub live: Vec<u64>,
}

impl Seg {
    pub fn is_live(&self, unit: usize) -> bool {
        self.live[unit / 64] >> (unit % 64) & 1 != 0
    }
    pub fn live_bytes(&self) -> usize {
        self.runs.iter().map(|r| r.1 as usize * 8).sum()
    }
}

#[derive(Clone)]
pub struct SpaceImg {
    pub nsegs: u32,
    pub bump: u32,
    pub vacant: Vec<u32>,
    pub segs: Vec<Seg>,
}

#[derive(Clone)]
pub struct Image {
    pub prefix: Vec<u8>,
    pub spaces: [SpaceImg; 2],
    pub vm: Vec<u8>,
    pub trailer: u64,
}

pub fn parse(full: &[u8]) -> Res<Image> {
    let bad = |w: &str| ["parse: ", w].concat();
    let (body, trailer) = full.split_last_chunk::<8>().ok_or(bad("short"))?;
    if body.len() < PREFIX || &body[..8] != b"MMSNAP\0\x01" {
        return Err(bad("magic"));
    }
    let mut r = Reader::new(&body[PREFIX..]);
    let mut space = || -> Res<SpaceImg> {
        let nsegs = r.u32().ok_or(bad("nsegs"))?;
        let bump = r.u32().ok_or(bad("bump"))?;
        let nvac = r.u32().ok_or(bad("nvac"))?;
        let vacant = (0..nvac)
            .map(|_| r.u32().ok_or(bad("vac")))
            .collect::<Res<_>>()?;
        let present = r.u32().ok_or(bad("present"))?;
        let mut segs = Vec::new();
        for _ in 0..present {
            let idx = r.u32().ok_or(bad("idx"))?;
            let large = r.bool().ok_or(bad("large"))?;
            let top = r.u32().ok_or(bad("top"))?;
            let nruns = r.u32().ok_or(bad("nruns"))?;
            let cap = if large {
                None
            } else {
                Some(r.u32().ok_or(bad("cap"))?)
            };
            let mut dense = vec![0u8; top as usize * 8];
            let mut live = vec![0u64; (top as usize).div_ceil(64)];
            let mut runs = Vec::new();
            for _ in 0..nruns {
                let off = r.u32().ok_or(bad("off"))?;
                let units = r.u32().ok_or(bad("units"))?;
                let data = r.take(units as usize * 8).ok_or(bad("data"))?;
                let at = off as usize * 8;
                dense
                    .get_mut(at..at + data.len())
                    .ok_or(bad("run"))?
                    .copy_from_slice(data);
                for u in off..off + units {
                    live[u as usize / 64] |= 1 << (u % 64);
                }
                runs.push((off, units));
            }
            segs.push(Seg {
                idx,
                large,
                top,
                cap,
                runs,
                dense,
                live,
            });
        }
        Ok(SpaceImg {
            nsegs,
            bump,
            vacant,
            segs,
        })
    };
    let words = space()?;
    let bytes = space()?;
    let vm = r.take(r.remaining()).unwrap_or_default().to_vec();
    Ok(Image {
        prefix: body[..PREFIX].to_vec(),
        spaces: [words, bytes],
        vm,
        trailer: u64::from_le_bytes(*trailer),
    })
}

fn u32s(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// The FULL image again, byte for byte.
pub fn emit(img: &Image) -> Vec<u8> {
    let mut out = img.prefix.clone();
    for s in &img.spaces {
        u32s(&mut out, s.nsegs);
        u32s(&mut out, s.bump);
        u32s(&mut out, s.vacant.len() as u32);
        for &v in &s.vacant {
            u32s(&mut out, v);
        }
        u32s(&mut out, s.segs.len() as u32);
        for g in &s.segs {
            u32s(&mut out, g.idx);
            out.push(u8::from(g.large));
            u32s(&mut out, g.top);
            u32s(&mut out, g.runs.len() as u32);
            if let Some(c) = g.cap {
                u32s(&mut out, c);
            }
            for &(off, units) in &g.runs {
                u32s(&mut out, off);
                u32s(&mut out, units);
                let at = off as usize * 8;
                out.extend_from_slice(&g.dense[at..at + units as usize * 8]);
            }
        }
    }
    out.extend_from_slice(&img.vm);
    out.extend_from_slice(&img.trailer.to_le_bytes());
    out
}

// ---------------------------------------------------------------------------
// The base-relative form.

const DELTA_MAGIC: &[u8; 8] = b"MMBREL\0\x01";
/// Block size meaning "whole segment".
pub const SEGMENT: u32 = u32::MAX;

/// The base id: 32 bytes of XXH64 over the base image (it embeds the build
/// id). A real format would use a cryptographic hash if bases can come
/// from untrusted stores; snapshots are trusted input here (spec §9.3).
pub fn base_id(base: &[u8]) -> [u8; 32] {
    let mut id = [0u8; 32];
    for (seed, part) in (0u64..).zip(id.chunks_mut(8)) {
        part.copy_from_slice(&xxh64(base, 0xB45E_0000 + seed).to_le_bytes());
    }
    id
}

pub struct Base {
    pub id: [u8; 32],
    pub img: Image,
    /// Per space: segment index -> position in `img.spaces[s].segs`.
    pub by_idx: [Vec<Option<usize>>; 2],
}

impl Base {
    pub fn new(bytes: &[u8]) -> Res<Base> {
        let img = parse(bytes)?;
        let by_idx = [0, 1].map(|s| {
            let mut v = vec![None; img.spaces[s].nsegs as usize];
            for (i, g) in img.spaces[s].segs.iter().enumerate() {
                if let Some(slot) = v.get_mut(g.idx as usize) {
                    *slot = Some(i);
                }
            }
            v
        });
        Ok(Base {
            id: base_id(bytes),
            img,
            by_idx,
        })
    }
    pub fn seg(&self, space: usize, idx: u32) -> Option<&Seg> {
        let i = (*self.by_idx[space].get(idx as usize)?)?;
        Some(&self.img.spaces[space].segs[i])
    }
}

fn block_len(block: u32, g: &Seg) -> usize {
    if block == SEGMENT {
        (g.top as usize * 8).max(8)
    } else {
        block as usize
    }
}

/// Live byte ranges of `g` inside `[lo, hi)`.
fn live_ranges(runs: &[(u32, u32)], lo: usize, hi: usize, f: &mut dyn FnMut(usize, usize)) {
    // Runs are sorted and disjoint: start at the first one ending after `lo`.
    let first = runs.partition_point(|&(off, units)| (off + units) as usize * 8 <= lo);
    for &(off, units) in &runs[first..] {
        let (a, b) = (off as usize * 8, (off + units) as usize * 8);
        if a >= hi {
            break;
        }
        let (a, b) = (a.max(lo), b.min(hi));
        if a < b {
            f(a, b);
        }
    }
}

/// Base id, block size, the target's prefix, segment tables and run lists
/// in full; per segment the changed blocks (live bytes only) against the
/// base's segment with the same index, or all live data when the base has
/// none; the VM section in full.
pub fn encode(full: &[u8], base: &Base, block: u32) -> Res<Vec<u8>> {
    let t = parse(full)?;
    let mut out = Vec::with_capacity(full.len() / 4);
    out.extend_from_slice(DELTA_MAGIC);
    out.extend_from_slice(&base.id);
    u32s(&mut out, block);
    out.extend_from_slice(&t.prefix);
    out.extend_from_slice(&t.trailer.to_le_bytes());
    for (si, s) in t.spaces.iter().enumerate() {
        u32s(&mut out, s.nsegs);
        u32s(&mut out, s.bump);
        u32s(&mut out, s.vacant.len() as u32);
        for &v in &s.vacant {
            u32s(&mut out, v);
        }
        u32s(&mut out, s.segs.len() as u32);
        for g in &s.segs {
            u32s(&mut out, g.idx);
            out.push(u8::from(g.large));
            u32s(&mut out, g.top);
            u32s(&mut out, g.cap.unwrap_or(0));
            u32s(&mut out, g.runs.len() as u32);
            for &(off, units) in &g.runs {
                u32s(&mut out, off);
                u32s(&mut out, units);
            }
            let Some(b) = base.seg(si, g.idx) else {
                out.push(0);
                live_ranges(&g.runs, 0, g.dense.len(), &mut |a, z| {
                    out.extend_from_slice(&g.dense[a..z])
                });
                continue;
            };
            out.push(1);
            let bl = block_len(block, g);
            let n = g.dense.len().div_ceil(bl);
            let mut bitmap = vec![0u8; n.div_ceil(8)];
            let mut data = Vec::new();
            for k in 0..n {
                let (lo, hi) = (k * bl, ((k + 1) * bl).min(g.dense.len()));
                let mut changed = false;
                live_ranges(&g.runs, lo, hi, &mut |a, z| {
                    let base_part = b.dense.get(a..z.min(b.dense.len())).unwrap_or(&[]);
                    changed |= base_part.len() != z - a || base_part != &g.dense[a..z];
                });
                if changed {
                    bitmap[k / 8] |= 1 << (k % 8);
                    live_ranges(&g.runs, lo, hi, &mut |a, z| {
                        data.extend_from_slice(&g.dense[a..z])
                    });
                }
            }
            out.extend_from_slice(&bitmap);
            out.extend_from_slice(&data);
        }
    }
    u32s(&mut out, t.vm.len() as u32);
    out.extend_from_slice(&t.vm);
    Ok(out)
}

/// The FULL image `encode` read, from the delta and the same base.
pub fn apply(delta: &[u8], base: &Base, check: bool) -> Res<Vec<u8>> {
    let bad = |w: &str| ["delta: ", w].concat();
    let mut r = Reader::new(delta);
    if r.take(8) != Some(DELTA_MAGIC.as_slice()) {
        return Err(bad("magic"));
    }
    if r.take(32) != Some(base.id.as_slice()) {
        return Err("wrong base: the delta was written against another base image".into());
    }
    let block = r.u32().ok_or(bad("block"))?;
    let prefix = r.take(PREFIX).ok_or(bad("prefix"))?.to_vec();
    let trailer = r.u64().ok_or(bad("trailer"))?;
    let mut spaces = Vec::new();
    for si in 0..2 {
        let nsegs = r.u32().ok_or(bad("nsegs"))?;
        let bump = r.u32().ok_or(bad("bump"))?;
        let nvac = r.count(4).ok_or(bad("nvac"))?;
        let vacant = (0..nvac)
            .map(|_| r.u32().ok_or(bad("vac")))
            .collect::<Res<_>>()?;
        let present = r.count(17).ok_or(bad("present"))?;
        let mut segs = Vec::with_capacity(present);
        for _ in 0..present {
            let idx = r.u32().ok_or(bad("idx"))?;
            let large = r.bool().ok_or(bad("large"))?;
            let top = r.u32().ok_or(bad("top"))?;
            let cap = r.u32().ok_or(bad("cap"))?;
            let nruns = r.count(8).ok_or(bad("nruns"))?;
            let runs: Vec<(u32, u32)> = (0..nruns)
                .map(|_| Ok((r.u32().ok_or(bad("off"))?, r.u32().ok_or(bad("units"))?)))
                .collect::<Res<_>>()?;
            let len = top as usize * 8;
            if runs
                .iter()
                .any(|&(o, u)| (o as usize + u as usize) * 8 > len)
            {
                return Err(bad("run"));
            }
            let mut g = Seg {
                idx,
                large,
                top,
                cap: (!large).then_some(cap),
                runs,
                dense: Vec::new(),
                live: Vec::new(),
            };
            match r.u8() {
                Some(0) => {
                    g.dense = vec![0; len];
                    let mut err = false;
                    live_ranges(&g.runs.clone(), 0, len, &mut |a, z| match r.take(z - a) {
                        Some(d) => g.dense[a..z].copy_from_slice(d),
                        None => err = true,
                    });
                    if err {
                        return Err(bad("data"));
                    }
                }
                Some(1) => {
                    let b = base.seg(si, idx).ok_or(bad("no base segment"))?;
                    g.dense = b.dense.clone();
                    g.dense.resize(len, 0);
                    let bl = block_len(block, &g);
                    let n = len.div_ceil(bl);
                    let bitmap = r.take(n.div_ceil(8)).ok_or(bad("bitmap"))?.to_vec();
                    let runs = g.runs.clone();
                    let mut err = false;
                    for k in 0..n {
                        if bitmap[k / 8] >> (k % 8) & 1 != 0 {
                            live_ranges(
                                &runs,
                                k * bl,
                                ((k + 1) * bl).min(len),
                                &mut |a, z| match r.take(z - a) {
                                    Some(d) => g.dense[a..z].copy_from_slice(d),
                                    None => err = true,
                                },
                            );
                        }
                    }
                    if err {
                        return Err(bad("data"));
                    }
                }
                _ => return Err(bad("mode")),
            }
            segs.push(g);
        }
        spaces.push(SpaceImg {
            nsegs,
            bump,
            vacant,
            segs,
        });
    }
    let vm = r.bytes().ok_or(bad("vm"))?.to_vec();
    let [words, bytes]: [SpaceImg; 2] = spaces.try_into().map_err(|_| bad("spaces"))?;
    let full = emit(&Image {
        prefix,
        spaces: [words, bytes],
        vm,
        trailer,
    });
    // The FULL checksum catches a wrong base that slipped past the id.
    let (body, _) = full.split_last_chunk::<8>().ok_or(bad("short"))?;
    if check && xxh64(body, 0) != trailer {
        return Err("checksum: the rebuilt image differs from the one encoded".into());
    }
    Ok(full)
}
