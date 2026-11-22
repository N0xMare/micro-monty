//! Heap snapshots: a restored heap is the heap the collection left, down to
//! where the next allocations land; malformed input is an error, never a
//! panic.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    reason = "tests"
)]

use mm_core::{Error, Heap, HeapConfig, Reader, Ref, Value, hash_bytes, types, xxh64};

const T: u16 = types::FIRST_ENGINE;

/// A heap with live and dead objects, strings, byte blocks and a large
/// object, collected with `keep` as roots.
fn sample() -> (Heap, Vec<Value>) {
    let mut heap = Heap::new(HeapConfig {
        memory_limit: 64 << 20,
        ..HeapConfig::default()
    });
    let mut keep = Vec::new();
    for i in 0..20_000u32 {
        let r = heap.alloc(T, 0, 2, 1).unwrap();
        heap.set(r, 0, Value::int(i64::from(i)).unwrap()).unwrap();
        if i % 3 == 0 {
            let s = heap.new_str(&format!("str-{i}-é")).unwrap();
            heap.set(r, 1, Value::from_ref(s, 0)).unwrap();
        }
        if i % 7 != 0 {
            keep.push(Value::from_ref(r, 0));
        }
        if i % 1000 == 0 {
            let name = heap.intern(&format!("name{i}")).unwrap();
            keep.push(Value::from_ref(name, 0));
        }
    }
    let big = heap.alloc(T + 1, 0, 40_000, 0).unwrap();
    keep.push(Value::from_ref(big, 0));
    let blob = heap.alloc_bytes(&vec![7u8; 300_000]).unwrap();
    let holder = heap.alloc(T, 0, 1, 0).unwrap();
    heap.set(holder, 0, Value::from_byte_ref(blob)).unwrap();
    keep.push(Value::from_ref(holder, 0));
    let roots = keep.clone();
    heap.collect(&mut |t| t.values(&roots));
    (heap, keep)
}

fn image(heap: &Heap) -> Vec<u8> {
    let mut out = Vec::new();
    heap.write_snapshot(&mut out);
    out
}

#[test]
fn restore_is_the_collected_heap() {
    let (mut heap, keep) = sample();
    // No spare commitment: restoring must preserve the ability to allocate
    // into existing free blocks without unexpectedly growing collector scratch.
    heap.set_memory_limit(heap.memory_limit() - heap.available_bytes());
    let bytes = image(&heap);
    let mut copy = Heap::read_snapshot(&mut Reader::new(&bytes), u64::MAX).unwrap();
    assert_eq!(image(&copy), bytes);
    assert_eq!(copy.committed_bytes(), heap.committed_bytes());
    assert_eq!(copy.stats(), heap.stats());
    for &v in &keep {
        let r = v.as_obj().unwrap();
        assert_eq!(copy.header(r), heap.header(r));
        assert_eq!(copy.slot_words(r), heap.slot_words(r));
    }
    assert_eq!(
        copy.find_interned("name5000"),
        heap.find_interned("name5000")
    );
    // Allocation continues identically: the free lists were rebuilt in order.
    for n in [1u32, 2, 5, 9, 70, 300, 3, 1] {
        for _ in 0..50 {
            let a = heap.alloc(T, 0, n, 0).unwrap();
            let b = copy.alloc(T, 0, n, 0).unwrap();
            assert_eq!(a, b);
        }
    }
    assert_eq!(heap.new_str("xyz"), copy.new_str("xyz"));
    // Far past the restored `top` (storage the restore did not initialize):
    // every slot written there reads back, and both heaps image alike.
    heap.set_memory_limit(64 << 20);
    copy.set_memory_limit(64 << 20);
    let mut more = Vec::new();
    for i in 0..30_000u32 {
        let a = heap.alloc(T, 0, 3, 0).unwrap();
        let b = copy.alloc(T, 0, 3, 0).unwrap();
        assert_eq!(a, b);
        for k in 0..3 {
            let v = Value::int(i64::from(i * 3 + k)).unwrap();
            heap.set(a, k, v).unwrap();
            copy.set(b, k, v).unwrap();
        }
        more.push(Value::from_ref(a, 0));
    }
    assert_eq!(
        copy.get(more[29_999].as_obj().unwrap(), 2),
        Ok(Value::int(89_999).unwrap())
    );
    let roots: Vec<Value> = keep.iter().chain(&more).copied().collect();
    heap.collect(&mut |t| t.values(&roots));
    copy.collect(&mut |t| t.values(&roots));
    assert_eq!(image(&copy), image(&heap));
}

#[test]
fn empty_heap_round_trips() {
    let heap = Heap::default();
    let bytes = image(&heap);
    let copy = Heap::read_snapshot(&mut Reader::new(&bytes), u64::MAX).unwrap();
    assert_eq!(image(&copy), bytes);
    // The restoring host's lower limit also applies to subsequent allocation.
    for limit in [0, 2 << 20] {
        let mut small = Heap::read_snapshot(&mut Reader::new(&bytes), limit).unwrap();
        assert_eq!(small.memory_limit(), limit);
        assert_eq!(small.alloc_bytes_zeroed(3 << 20), Err(Error::OutOfMemory));
    }
}

#[test]
fn truncated_and_corrupt_images_are_errors() {
    let (heap, _) = sample();
    let bytes = image(&heap);
    for cut in (0..bytes.len()).step_by(bytes.len() / 500 + 1) {
        let r = Heap::read_snapshot(&mut Reader::new(&bytes[..cut]), u64::MAX);
        assert!(r.is_err(), "cut at {cut}");
    }
    // Flipped bytes: an error or a heap, never a panic.
    let mut x = 0x1234_5678_u64;
    for _ in 0..300 {
        x = xxh64(&x.to_le_bytes(), 1);
        let mut b = bytes.clone();
        let at = (x as usize) % b.len().min(4096);
        b[at] ^= (x >> 32) as u8 | 1;
        let _ = Heap::read_snapshot(&mut Reader::new(&b), 1 << 30);
    }
}

/// A word-space segment of an image: where its index, top, run count and
/// capacity (a large segment's room) are, and its runs as (unit offset,
/// units, where their data starts).
struct Segment {
    index: usize,
    large: bool,
    top: usize,
    nruns: usize,
    cap: usize,
    runs: Vec<(usize, usize, usize)>,
}

/// The word space's present segments, read from `b` by the image layout.
fn segments(b: &[u8]) -> Vec<Segment> {
    let u32_at = |at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    // Limits, GC ratio and minimum, five stats, the count a forced
    // collection kept (`kept_since_gc`) and the intern table ref.
    let space = 8 + 4 + 8 + 5 * 8 + 8 + 4;
    let nvacant = u32_at(space + 8);
    let present = u32_at(space + 12 + 4 * nvacant);
    let mut at = space + 16 + 4 * nvacant;
    let mut out = Vec::new();
    for _ in 0..present {
        let large = b[at + 4] != 0;
        let mut seg = Segment {
            index: at,
            large,
            top: at + 5,
            nruns: at + 9,
            cap: at + 13,
            runs: Vec::new(),
        };
        at += 17;
        for _ in 0..u32_at(seg.nruns) {
            let (off, units) = (u32_at(at), u32_at(at + 4));
            seg.runs.push((off, units, at + 8));
            at += 8 + units * 8;
        }
        out.push(seg);
    }
    out
}

/// Where the word `i` words into the object `raw` refers to is in `b`.
fn word_at(b: &[u8], raw: u32) -> impl Fn(usize) -> usize {
    let (seg, off) = ((raw >> 17) as usize, (raw & 0x1_ffff) as usize);
    let u32_at = |at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    let data = segments(b)
        .into_iter()
        .filter(|s| u32_at(s.index) == seg)
        .flat_map(|s| s.runs)
        .find(|&(o, units, _)| (o..o + units).contains(&off))
        .map(|(o, _, data)| data + (off - o) * 8)
        .unwrap();
    move |i| data + i * 8
}

#[test]
fn restore_refuses_each_inconsistent_field() {
    let (heap, keep) = sample();
    let bytes = image(&heap);
    let restores = |b: &[u8]| Heap::read_snapshot(&mut Reader::new(b), u64::MAX).is_ok();
    assert!(restores(&bytes));
    let seg = segments(&bytes)
        .into_iter()
        .find(|s| !s.large && !s.runs.is_empty())
        .unwrap();
    let (idx, top, nruns, cap, run) = (seg.index, seg.top, seg.nruns, seg.cap, seg.runs[0].2 - 8);
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let with = |at: usize, v: u32| {
        let mut b = bytes.clone();
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        b
    };
    let replaced = |from: &[u8], to: &[u8]| {
        let at = bytes.windows(from.len()).position(|w| w == from).unwrap();
        let mut b = bytes.clone();
        b[at..at + to.len()].copy_from_slice(to);
        b
    };
    let other = keep[2].as_obj().unwrap().raw();
    assert!(u32_at(run + 4) > 0 && u32_at(nruns) > 0);
    for (what, b) in [
        ("no segments", with(72, 0)),
        ("too many segments", with(72, u32::MAX)),
        ("another object as the intern table", with(68, other)),
        ("segment index 0", with(idx, 0)),
        ("capacity not a power of two", with(cap, u32_at(cap) - 1)),
        ("top past the capacity", with(top, u32_at(cap) + 1)),
        ("an empty run", with(run + 4, 0)),
        ("a run past top", with(run, u32_at(top))),
        ("a block of no type", with(run + 8, 0)),
        (
            "invalid UTF-8",
            replaced("str-3-é".as_bytes(), b"str-3-\xff\xfe"),
        ),
        (
            "a wrong char count",
            replaced("str-3-é".as_bytes(), b"str-3-ab"),
        ),
        ("a wrong cached hash", replaced(b"name5000", b"name5001")),
        ("a wrong intern count", {
            // Slot 0 of the intern table holds how many names it has.
            let at = word_at(&bytes, u32_at(68))(1);
            let mut b = bytes.clone();
            b[at..at + 8].copy_from_slice(&Value::int(0).unwrap().to_bits().to_le_bytes());
            b
        }),
        ("an intern table with no free slot", {
            // Every slot names an interned string and the count agrees: a
            // lookup of a new name would probe forever.
            let table = Ref::from_raw(u32_at(68));
            let len = heap.header(table).unwrap().len();
            let name = (1..len)
                .map(|i| heap.get(table, i).unwrap())
                .find(|&v| v != Value::UNDEFINED)
                .unwrap();
            let word = word_at(&bytes, u32_at(68));
            let mut b = bytes.clone();
            let count = Value::int(i64::from(len - 1)).unwrap();
            b[word(1)..word(1) + 8].copy_from_slice(&count.to_bits().to_le_bytes());
            for i in 1..len as usize {
                b[word(i + 1)..word(i + 1) + 8].copy_from_slice(&name.to_bits().to_le_bytes());
            }
            b
        }),
    ] {
        assert!(!restores(&b), "restored with {what}");
    }
}

/// The entries of intern table `table`, sorted.
fn interned_entries(heap: &Heap, table: Ref) -> Vec<u64> {
    let cap = heap.header(table).unwrap().len() - 1;
    let mut entries: Vec<u64> = (1..=cap)
        .map(|i| heap.get(table, i).unwrap().to_bits())
        .filter(|&w| w != Value::UNDEFINED.to_bits())
        .collect();
    entries.sort_unstable();
    entries
}

/// Tables of `cap` slots a restore admits, made of the entries `present`
/// (`honest` is the table they came from): none has them where lookups
/// look.
fn forged_tables(
    honest: &[u64],
    present: &[u64],
    cap: usize,
    hash: &dyn Fn(u64) -> u64,
) -> Vec<(&'static str, Vec<u64>)> {
    let free = Value::UNDEFINED.to_bits();
    let mut reversed = honest.to_vec();
    reversed.reverse();
    let mut across_end = vec![free; cap];
    let mut every_other = vec![free; cap];
    for (k, &w) in present.iter().enumerate() {
        across_end[(cap - 150 + k) % cap] = w;
        every_other[2 * k + 1] = w;
    }
    let mut from_start = vec![free; cap];
    from_start[..present.len()].copy_from_slice(present);
    let one_name = honest
        .iter()
        .map(|&w| if w == free { free } else { present[7] })
        .collect();
    let mut half_full = vec![free; cap];
    for (k, slot) in half_full.iter_mut().take(cap / 2).enumerate() {
        *slot = present[k % present.len()];
    }
    // Runs of 40 with a free slot between, buckets falling from run to
    // run (`honest` holds them rising): a doubling that sorts each run
    // alone places them out of order.
    let mut falling_runs = vec![free; cap];
    let rising = honest.iter().copied().filter(|&w| w != free);
    for (k, w) in rising.rev().enumerate() {
        falling_runs[k + k / 40] = w;
    }
    // A doubling's two runs meet: from slot 1 on, entries of the first
    // buckets whose hashes send them to the upper half, then one of the
    // last buckets that stays in the lower half; after a free slot, every
    // other entry that stays there. Sorted run by run, those pile up
    // behind the one of the last buckets, into the upper half's first
    // slots, which are taken.
    let upper = |w: u64| hash(w) & cap as u64 != 0;
    let bucket = |w: u64| hash(w) & (cap as u64 - 1);
    let first: Vec<u64> = present
        .iter()
        .copied()
        .filter(|&w| upper(w) && (1..100).contains(&bucket(w)))
        .collect();
    let high = present
        .iter()
        .copied()
        .find(|&w| !upper(w) && bucket(w) >= 1000)
        .unwrap();
    let rest: Vec<u64> = present
        .iter()
        .copied()
        .filter(|&w| !upper(w) && w != high)
        .collect();
    assert!(first.len() >= 10 && rest.len() >= 150);
    let mut runs_meet = vec![free; cap];
    let run = first.iter().chain([&high, &free]).chain(&rest);
    for (slot, &w) in runs_meet[1..].iter_mut().zip(run) {
        *slot = w;
    }
    vec![
        ("two runs that meet", runs_meet),
        ("runs of falling buckets", falling_runs),
        ("reversed", reversed),
        ("one run across the end", across_end),
        ("one run from the start", from_start),
        ("every other slot", every_other),
        ("one name in every entry", one_name),
        ("half full of repeats", half_full),
    ]
}

/// A restore admits any intern table whose entries are interned strings,
/// as many as its count says and at most half its slots, wherever they
/// lie: a forged one's are not where lookups look. Pruning and growing
/// such a table still keep exactly its live entries, each as often as it
/// was there.
#[test]
fn a_forged_intern_table_prunes_and_grows_to_its_live_entries() {
    let mut heap = Heap::new(HeapConfig {
        memory_limit: 64 << 20,
        ..HeapConfig::default()
    });
    let names: Vec<String> = (0..400).map(|i| format!("forged{i}")).collect();
    let refs: Vec<Ref> = names.iter().map(|n| heap.intern(n).unwrap()).collect();
    // Entries carry no kind; neither do these roots.
    let roots: Vec<Value> = refs.iter().map(|&r| Value::from_ref(r, 0)).collect();
    heap.collect(&mut |t| t.values(&roots));
    let bytes = image(&heap);
    let table = Ref::from_raw(u32::from_le_bytes(bytes[68..72].try_into().unwrap()));
    let cap = heap.header(table).unwrap().len() as usize - 1;
    let free = Value::UNDEFINED.to_bits();
    let honest: Vec<u64> = (1..=cap as u32)
        .map(|i| heap.get(table, i).unwrap().to_bits())
        .collect();
    let present = interned_entries(&heap, table);
    assert_eq!((cap, present.len()), (1024, 400));
    let keeps: [fn(usize) -> bool; 4] = [|_| false, |_| true, |i| i % 3 == 0, |i| i % 3 != 0];
    let hash_of = |w: u64| {
        let i = roots.iter().position(|v| v.to_bits() == w).unwrap();
        hash_bytes(names[i].as_bytes())
    };
    for (what, arranged) in forged_tables(&honest, &present, cap, &hash_of) {
        let word = word_at(&bytes, table.raw());
        let mut b = bytes.clone();
        let count = arranged.iter().filter(|&&w| w != free).count();
        let count = Value::int(i64::try_from(count).unwrap()).unwrap().to_bits();
        b[word(1)..word(1) + 8].copy_from_slice(&count.to_le_bytes());
        for (k, w) in arranged.iter().enumerate() {
            b[word(k + 2)..word(k + 2) + 8].copy_from_slice(&w.to_le_bytes());
        }
        for (k, keep) in keeps.into_iter().enumerate() {
            let restored = Heap::read_snapshot(&mut Reader::new(&b), u64::MAX);
            assert!(restored.is_ok(), "{what}: refused");
            let mut copy = restored.unwrap();
            let kept: Vec<Value> = (0..400).filter(|&i| keep(i)).map(|i| roots[i]).collect();
            copy.collect(&mut |t| t.values(&kept));
            let mut want: Vec<u64> = arranged
                .iter()
                .copied()
                .filter(|&w| kept.iter().any(|v| v.to_bits() == w))
                .collect();
            want.sort_unstable();
            assert_eq!(interned_entries(&copy, table), want, "{what}, keep {k}");
            assert_eq!(copy.interned_count() as usize, want.len(), "{what}, {k}");
            // Lookups end, whatever they find.
            for name in &names {
                let _ = copy.find_interned(name);
            }
            // Doubled, the table holds the same entries.
            copy.reserve_interned(cap as u32).unwrap();
            let mut grown = None;
            copy.for_each_object(&mut |r, h| {
                if h.type_id() == types::INTERN_TABLE && h.len() as usize == 2 * cap + 1 {
                    grown = Some(r);
                }
            });
            assert_eq!(interned_entries(&copy, grown.unwrap()), want, "{what}, {k}");
            assert_eq!(copy.interned_count() as usize, want.len(), "{what}, {k}");
            let new = copy.intern("a new name").unwrap();
            assert_eq!(copy.find_interned("a new name"), Some(new), "{what}, {k}");
        }
    }
}

#[test]
fn a_large_blocks_room_is_restored_and_checked() {
    let mut heap = Heap::new(HeapConfig {
        memory_limit: 64 << 20,
        ..HeapConfig::default()
    });
    let chunk = vec![b'x'; 300_000];
    let room = 1 << 20;
    let b = heap.alloc_bytes(&[]).unwrap();
    let b = heap.append_bytes(b, &chunk, room).unwrap();
    let holder = heap.alloc(T, 0, 1, 0).unwrap();
    heap.set(holder, 0, Value::from_byte_ref(b)).unwrap();
    let roots = [Value::from_ref(holder, 0)];
    heap.collect(&mut |t| t.values(&roots));
    let bytes = image(&heap);
    // The room is a number: the image holds the block's bytes only.
    assert!(bytes.len() < chunk.len() + (64 << 10));
    let mut copy = Heap::read_snapshot(&mut Reader::new(&bytes), u64::MAX).unwrap();
    let b = copy.get(holder, 0).unwrap().as_byte_ref().unwrap();
    let committed = copy.committed_bytes();
    assert_eq!(copy.append_bytes(b, &chunk, room), Ok(b));
    assert_eq!(copy.committed_bytes(), committed);
    // The room record follows the block's top and its one run.
    let (top, cap) = (1 + chunk.len().div_ceil(8), 1 + room.div_ceil(8));
    let field: Vec<u8> = [top, 1, cap]
        .iter()
        .flat_map(|&v| (v as u32).to_le_bytes())
        .collect();
    let at = bytes.windows(12).position(|w| w == field).unwrap() + 8;
    let with = |v: usize| {
        let mut b = bytes.clone();
        b[at..at + 4].copy_from_slice(&(v as u32).to_le_bytes());
        b
    };
    let restore = |b: &[u8], avail: u64| Heap::read_snapshot(&mut Reader::new(b), avail).err();
    let largest = 1 + mm_core::MAX_OBJECT_BYTES as usize / 8;
    assert_eq!(restore(&with(top - 1), u64::MAX), Some(Error::BadSnapshot));
    assert_eq!(
        restore(&with(largest + 1), u64::MAX),
        Some(Error::BadSnapshot)
    );
    assert_eq!(
        restore(&with(u32::MAX as usize), u64::MAX),
        Some(Error::BadSnapshot)
    );
    // A room the limit cannot admit is refused before it is reserved.
    assert_eq!(restore(&with(largest), 32 << 20), Some(Error::OutOfMemory));
    assert_eq!(restore(&with(top), 32 << 20), None);
}

#[test]
fn commitment_is_bounded() {
    let (heap, _) = sample();
    let bytes = image(&heap);
    let r = Heap::read_snapshot(&mut Reader::new(&bytes), 1 << 20);
    assert_eq!(r.err(), Some(Error::OutOfMemory));
    // Fitting the arena alone is insufficient: collection must be possible
    // without allocating beyond the restoring host's limit.
    let r = Heap::read_snapshot(&mut Reader::new(&bytes), heap.committed_bytes());
    assert_eq!(r.err(), Some(Error::OutOfMemory));
}

#[test]
fn restored_refs_are_checked() {
    let (heap, _) = sample();
    let bytes = image(&heap);
    let copy = Heap::read_snapshot(&mut Reader::new(&bytes), u64::MAX).unwrap();
    assert_eq!(copy.header(Ref::from_raw(12345)), Err(Error::BadRef));
}

#[test]
fn xxh64_matches_reference_vectors() {
    assert_eq!(xxh64(b"", 0), 0xEF46_DB37_51D8_E999);
    assert_eq!(xxh64(b"a", 0), 0xD24E_C4F1_A98C_6E5B);
    assert_eq!(xxh64(b"abc", 0), 0x44BC_2CF5_AD77_0999);
    assert_eq!(
        xxh64(b"Nobody inspects the spammish repetition", 0),
        0xFBCE_A83C_8A37_8BF1
    );
    // Tails with a whole 8-byte word after the 32-byte stripes, and alone
    // (python-xxhash 3.x).
    assert_eq!(
        xxh64(b"Nobody inspects the spammish repetition!", 0),
        0xE47E_9A81_618E_DB79
    );
    assert_eq!(xxh64(b"0123456789ab", 0), 0x862E_2923_26B8_A4FC);
}
