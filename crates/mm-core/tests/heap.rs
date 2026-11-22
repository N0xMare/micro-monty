//! Arena, byte space, write barrier and string tests.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    reason = "tests"
)]

use mm_core::{
    ByteRef, Error, Heap, HeapConfig, INTERN_WALK_CAP, LARGE_OBJECT_BYTES, ObjectCursor, Ref,
    SEGMENT_BYTES, Value, flags, hash_bytes, hash_bytes_const, hash_tag, types,
};

const T: u16 = types::FIRST_ENGINE;
/// The word space's first segment: 64 KiB of data and two 1 KiB bitmaps.
const FIRST_COST: u64 = 64 * 1024 + 2048;

#[test]
fn objects_slots_and_headers() {
    let mut heap = Heap::default();
    assert_eq!(heap.committed_bytes(), 0);
    let r = heap.alloc(T + 3, flags::FROZEN, 2, 3).unwrap();
    let h = heap.header(r).unwrap();
    assert_eq!(
        (h.type_id(), h.flags(), h.nvals(), h.len()),
        (T + 3, flags::FROZEN, 2, 5)
    );
    assert_eq!(heap.get(r, 0), Ok(Value::UNDEFINED));
    assert_eq!(heap.get_raw(r, 4), Ok(0));
    heap.set(r, 1, Value::TRUE).unwrap();
    heap.set_raw(r, 2, u64::MAX).unwrap();
    assert_eq!(heap.get(r, 1), Ok(Value::TRUE));
    assert_eq!(heap.get_raw(r, 2), Ok(u64::MAX));
    // Slot kinds are enforced.
    assert_eq!(heap.get(r, 2), Err(Error::BadSlot));
    assert_eq!(heap.get_raw(r, 1), Err(Error::BadSlot));
    assert_eq!(heap.set(r, 2, Value::NONE), Err(Error::BadSlot));
    assert_eq!(heap.set_raw(r, 5, 0), Err(Error::BadSlot));
    let mut two = [Value::NONE; 2];
    heap.read_values(r, 0, &mut two).unwrap();
    assert_eq!(two, [Value::UNDEFINED, Value::TRUE]);
    assert_eq!(heap.read_values(r, 1, &mut two), Err(Error::BadSlot));
    heap.set_flags(r, flags::HAS_DICT).unwrap();
    assert_eq!(heap.header(r).unwrap().flags(), flags::HAS_DICT);
    // All-value and empty objects.
    let e = heap.alloc(T, 0, 0, 0).unwrap();
    assert!(heap.header(e).unwrap().is_empty());
    assert_eq!(heap.get(e, 0), Err(Error::BadSlot));
    let v = heap.alloc_init(T, 0, &[Value::NONE; 300], &[]).unwrap();
    assert_eq!(heap.header(v).unwrap().nvals(), 300);
    // Value slots move in place, overlapping either way, never into raws.
    let moved = heap.alloc(T, 0, 4, 1).unwrap();
    let vals = [Value::NONE, Value::TRUE, Value::FALSE, Value::UNDEFINED];
    heap.set_values(moved, 0, &vals).unwrap();
    heap.move_values(moved, 0, 1, 3).unwrap();
    let mut four = [Value::UNDEFINED; 4];
    heap.read_values(moved, 0, &mut four).unwrap();
    assert_eq!(four, [Value::NONE, Value::NONE, Value::TRUE, Value::FALSE]);
    heap.move_values(moved, 2, 0, 2).unwrap();
    heap.read_values(moved, 0, &mut four).unwrap();
    assert_eq!(four, [Value::TRUE, Value::FALSE, Value::TRUE, Value::FALSE]);
    // A short move down over many chunks (`pop(0)` on a long list).
    let ints: Vec<Value> = (0..600).map(|i| Value::int(i).unwrap()).collect();
    let long = heap.alloc_init(T, 0, &ints, &[]).unwrap();
    heap.move_values(long, 3, 1, 597).unwrap();
    let mut got = vec![Value::UNDEFINED; 600];
    heap.read_values(long, 0, &mut got).unwrap();
    let want: Vec<Value> = (0..1)
        .chain(3..600)
        .chain(598..600)
        .map(|i| Value::int(i).unwrap())
        .collect();
    assert_eq!(got, want);
    assert_eq!(heap.move_values(moved, 0, 1, 4), Err(Error::BadSlot));
    assert_eq!(heap.move_values(moved, u32::MAX, 0, 1), Err(Error::BadSlot));
    assert_eq!(heap.alloc(T, 0, 300, 1), Err(Error::BadType));
    assert_eq!(heap.alloc(T - 1, 0, 1, 0), Err(Error::BadType));
}

#[test]
fn object_traversal_allows_slot_writes_across_sparse_segments() {
    let mut heap = Heap::default();
    assert!(heap.next_object(&mut ObjectCursor::default()).is_none());
    let mut kept = Vec::new();
    for i in 0..1500 {
        let r = heap
            .alloc(T, 0, 1, if i % 3 == 0 { 2048 } else { 0 })
            .unwrap();
        heap.set(r, 0, Value::int(i).unwrap()).unwrap();
        if i % 7 == 0 {
            kept.push((r, i));
        }
    }
    heap.collect(&mut |t| {
        for &(r, _) in &kept {
            t.value(Value::from_ref(r, 0));
        }
    });
    kept.sort_unstable_by_key(|&(r, _)| r.raw());
    assert!(kept.last().unwrap().0.segment() > kept[0].0.segment());
    let mut cursor = ObjectCursor::default();
    for &(r, i) in &kept {
        let (found, header) = heap.next_object(&mut cursor).unwrap();
        assert_eq!(found, r);
        assert_eq!(header.nvals(), 1);
        assert_eq!(heap.get(r, 0), Ok(Value::int(i).unwrap()));
        heap.set(r, 0, Value::int(i + 1).unwrap()).unwrap();
    }
    assert!(heap.next_object(&mut cursor).is_none());
    assert!(heap.next_object(&mut cursor).is_none());
    for (r, i) in kept {
        assert_eq!(heap.get(r, 0), Ok(Value::int(i + 1).unwrap()));
    }
}

#[test]
fn forged_refs_are_errors() {
    let mut heap = Heap::default();
    let r = heap.alloc(T, 0, 4, 0).unwrap();
    let raw = r.raw();
    for bad in [0, 1, raw + 1, raw + 4, raw + 5, u32::MAX, 5 << 17, raw - 1] {
        let b = Ref::from_raw(bad);
        assert_eq!(heap.header(b), Err(Error::BadRef), "{bad:#x}");
        assert_eq!(heap.get(b, 0), Err(Error::BadRef));
        assert_eq!(heap.set(b, 0, Value::NONE), Err(Error::BadRef));
        assert_eq!(heap.set_raw(b, 0, 0), Err(Error::BadRef));
        assert_eq!(heap.set_flags(b, 0), Err(Error::BadRef));
        assert_eq!(heap.str_bytes(b), Err(Error::BadRef));
        assert_eq!(heap.bytes(ByteRef::from_raw(bad)), Err(Error::BadRef));
        assert_eq!(heap.intern_str(b), Err(Error::BadRef));
    }
    assert_eq!(heap.str_bytes(r), Err(Error::BadType));
    // A ref kept across a collection that freed its object is stale.
    heap.collect(&mut |_| {});
    assert_eq!(heap.get(r, 0), Err(Error::BadRef));
}

#[test]
fn byte_blocks_live_while_owned() {
    let mut heap = Heap::default();
    let b = heap.alloc_bytes(b"hello, world").unwrap();
    assert_eq!(heap.bytes(b).unwrap(), b"hello, world");
    heap.bytes_mut(b).unwrap()[0] = b'H';
    assert_eq!(heap.bytes(b).unwrap(), b"Hello, world");
    let empty = heap.alloc_bytes(b"").unwrap();
    assert_eq!(heap.bytes(empty).unwrap(), b"");
    let owner = heap
        .alloc_init(T, 0, &[Value::from_byte_ref(b)], &[])
        .unwrap();
    let root = Value::from_ref(owner, 0);
    let stats = heap.collect(&mut |t| t.value(root));
    assert_eq!(stats.live_bytes, 16 + 8 + 16);
    assert_eq!(heap.bytes(b).unwrap(), b"Hello, world");
    assert_eq!(heap.bytes(empty), Err(Error::BadRef));
    // The owner dies, and the block with it.
    heap.collect(&mut |_| {});
    assert_eq!(heap.bytes(b), Err(Error::BadRef));
    // Reused memory is zero-padded and zero-filled.
    let z = heap.alloc_bytes_zeroed(13).unwrap();
    assert_eq!(heap.bytes(z).unwrap(), &[0; 13]);
}

#[test]
fn copy_bytes_within_and_across_segments() {
    let mut heap = Heap::default();
    let a = heap.alloc_bytes(b"abcdefgh").unwrap();
    // Overlapping, forwards and backwards, inside one block.
    heap.copy_bytes(a, 0, a, 2, 5).unwrap();
    assert_eq!(heap.bytes(a).unwrap(), b"ababcdeh");
    heap.copy_bytes(a, 3, a, 0, 5).unwrap();
    assert_eq!(heap.bytes(a).unwrap(), b"bcdehdeh");
    // A dedicated segment and the first one, both ways.
    let big = heap
        .alloc_bytes_zeroed(LARGE_OBJECT_BYTES as usize)
        .unwrap();
    assert_ne!(big.segment(), a.segment());
    heap.copy_bytes(a, 1, big, 100, 3).unwrap();
    assert_eq!(&heap.bytes(big).unwrap()[99..104], b"\0cde\0");
    heap.copy_bytes(big, 99, a, 4, 4).unwrap();
    assert_eq!(heap.bytes(a).unwrap(), b"bcde\0cde");
    // Ranges are checked before anything is written; empty copies work.
    let before = heap.bytes(a).unwrap().to_vec();
    assert_eq!(heap.copy_bytes(a, 5, a, 0, 4), Err(Error::BadRef));
    assert_eq!(heap.copy_bytes(big, 0, a, 6, 3), Err(Error::BadRef));
    assert_eq!(heap.copy_bytes(a, usize::MAX, a, 0, 2), Err(Error::BadRef));
    assert_eq!(heap.bytes(a).unwrap(), before);
    heap.copy_bytes(a, 8, a, 0, 0).unwrap();
    assert_eq!(
        heap.copy_bytes(ByteRef::from_raw(7), 0, a, 0, 1),
        Err(Error::BadRef)
    );
}

#[test]
fn large_objects_get_dedicated_segments() {
    let mut heap = Heap::default();
    let slots = (LARGE_OBJECT_BYTES / 8) as u32;
    let big = heap.alloc(T, 0, slots, 0).unwrap();
    assert_eq!(big.raw() & ((1 << 17) - 1), 0);
    heap.set(big, slots - 1, Value::TRUE).unwrap();
    let data = vec![7u8; 3 * SEGMENT_BYTES as usize + 5];
    let blob = heap.alloc_bytes(&data).unwrap();
    assert_eq!(heap.bytes(blob).unwrap(), &data[..]);
    let s = "\u{e9}".repeat(1 << 20);
    let text = heap.new_str(&s).unwrap();
    assert_eq!(heap.as_str(text).unwrap(), s);
    let committed = heap.committed_bytes();
    assert!(committed > 5 * SEGMENT_BYTES);
    let roots = [Value::from_ref(big, 0), Value::from_byte_ref(blob)];
    heap.collect(&mut |t| t.values(&roots));
    assert_eq!(heap.get(big, slots - 1), Ok(Value::TRUE));
    assert_eq!(heap.as_str(text), Err(Error::BadRef));
    heap.collect(&mut |_| {});
    // Dead dedicated segments are returned at once; the word space keeps its
    // bump segment, still at its first size (the byte space never needed
    // one).
    assert_eq!(heap.committed_bytes(), FIRST_COST);
    assert_eq!(heap.header(big), Err(Error::BadRef));
}

#[test]
fn first_segments_grow_without_moving_refs() {
    let mut heap = Heap::default();
    // 30,000 four-unit objects (120,000 units) fit one 1 MiB segment, which
    // starts at 32 KiB and doubles five times while they are allocated.
    let mut objs = Vec::new();
    let mut blobs = Vec::new();
    let mut prev = Value::NONE;
    for i in 0..30_000u64 {
        let r = heap.alloc_init(T, 0, &[prev, Value::NONE], &[i]).unwrap();
        prev = Value::from_ref(r, 0);
        objs.push(r);
        if i % 30 == 0 {
            let b = heap.alloc_bytes(&[i as u8; 100]).unwrap();
            heap.set(r, 1, Value::from_byte_ref(b)).unwrap();
            blobs.push((i, b));
        }
    }
    assert!(objs.iter().all(|r| r.segment() == 1));
    // 1,000 blocks of 14 units grow the byte space from 4,096 to 16,384.
    let full = SEGMENT_BYTES + SEGMENT_BYTES / 32;
    let byte_space = 16_384 * 8 + 16_384 / 32 * 8;
    assert_eq!(heap.committed_bytes(), full + byte_space);
    let check = |heap: &Heap| {
        for (i, r) in objs.iter().enumerate() {
            assert_eq!(heap.get_raw(*r, 2), Ok(i as u64), "object {i}");
        }
        for &(i, b) in &blobs {
            assert_eq!(heap.bytes(b).unwrap(), &[i as u8; 100]);
        }
    };
    check(&heap);
    // Marks and start bits grew with the data: the whole chain survives.
    let root = [prev];
    let stats = heap.collect(&mut |t| t.values(&root));
    assert_eq!(stats.freed_bytes, 0);
    check(&heap);
    // A restore commits the same capacities.
    let mut snap = Vec::new();
    heap.write_snapshot(&mut snap);
    let copy = Heap::read_snapshot(&mut mm_core::Reader::new(&snap), u64::MAX).unwrap();
    assert_eq!(copy.committed_bytes(), heap.committed_bytes());
    check(&copy);
}

#[test]
fn a_growing_first_segment_stays_within_the_limit() {
    // The first segment doubles to 128 KiB within the limit; doubling
    // again would commit past it, so allocation stops there. (Objects of
    // 512 bytes: the mark stack is not what runs out.)
    let limit = 200 << 10;
    let mut heap = Heap::new(HeapConfig {
        memory_limit: limit,
        ..HeapConfig::default()
    });
    let mut n = 0u32;
    while heap.alloc(T, 0, 0, 63).is_ok() {
        n += 1;
    }
    assert!(n > 128, "{n} objects: the first segment grew");
    assert!(
        heap.committed_bytes() <= limit,
        "{} committed over a limit of {limit}",
        heap.committed_bytes()
    );
}

#[test]
fn segment_metadata_shares_the_budget_and_survives_exhaustion() {
    // Byte-only allocation needs no mark stack: the reservation outside the
    // committed segment must be the segment tables and vacant-index storage.
    let mut heap = Heap::default();
    heap.alloc_bytes_zeroed(LARGE_OBJECT_BYTES as usize)
        .unwrap();
    assert_eq!(
        heap.set_external_bytes(heap.memory_limit() - heap.committed_bytes()),
        Err(Error::OutOfMemory)
    );
    heap.set_external_bytes(heap.available_bytes()).unwrap();
    for _ in 0..32 {
        heap.collect(&mut |_| {});
        // Releasing and reusing an index must work with no spare budget for
        // growing the vacant-index table during collection.
        heap.alloc_bytes_zeroed(LARGE_OBJECT_BYTES as usize)
            .unwrap();
    }
}

#[test]
fn strings() {
    let mut heap = Heap::default();
    for s in [
        "",
        "a",
        "hello",
        "h\u{e9}llo \u{1d11e}",
        "12345678",
        "123456789",
    ] {
        let r = heap.new_str(s).unwrap();
        assert_eq!(heap.as_str(r).unwrap(), s);
        assert_eq!(heap.str_bytes(r).unwrap(), s.as_bytes());
        assert_eq!(heap.str_chars(r), Ok(s.chars().count() as u64));
        assert_eq!(heap.str_hash(r), Ok(hash_bytes(s.as_bytes())));
        let h = heap.header(r).unwrap();
        assert_eq!(h.type_id(), types::STR);
        assert_eq!(h.flags() & flags::ASCII != 0, s.is_ascii());
        assert_eq!(h.flags() & flags::INTERNED, 0);
        // Strings are read-only to the engine.
        assert_eq!(heap.set(r, 0, Value::NONE), Err(Error::BadType));
        assert_eq!(heap.set_raw(r, 2, 0), Err(Error::BadType));
    }
}

#[test]
fn char_positions_agree_with_char_indices() {
    // Mixed widths, so the eight-byte steps of a walk end on every kind of
    // byte; accesses interleave strings and jump around, so walks start
    // from either end and from remembered positions in both directions.
    let mut heap = Heap::default();
    let mut seed = 7u64;
    let mut next = |n: usize| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        (seed >> 33) as usize % n
    };
    let alphabet = [
        'a',
        '\u{e9}',
        '\u{20ac}',
        '\u{1f600}',
        '\u{7f}',
        '\u{80}',
        '\u{ffff}',
    ];
    let mut strs = Vec::new();
    for len in [1, 2, 7, 8, 9, 15, 16, 17, 63, 64, 65, 300] {
        let s: String = (0..len).map(|_| alphabet[next(alphabet.len())]).collect();
        strs.push((heap.new_str(&s).unwrap(), s));
    }
    for _ in 0..20_000 {
        let (r, s) = &strs[next(strs.len())];
        let starts: Vec<usize> = s.char_indices().map(|(b, _)| b).collect();
        let k = next(starts.len() + 2);
        let at = starts.get(k).copied().unwrap_or(s.len());
        assert_eq!(heap.str_offset(*r, k).unwrap().0, at, "{s:?}[{k}]");
        let b = next(s.len() + 2);
        let chars = s.as_bytes()[..b.min(s.len())]
            .iter()
            .filter(|&&x| x & 0xC0 != 0x80)
            .count();
        assert_eq!(heap.str_index(*r, b).unwrap().0, chars, "{s:?} byte {b}");
    }
}

#[test]
fn char_positions_never_outlive_their_string() {
    // A position remembered in one string must not be used for another
    // allocated at its address after a collection.
    let mut heap = Heap::default();
    let old = heap.new_str(&"\u{20ac}".repeat(30)).unwrap();
    assert_eq!(heap.str_offset(old, 10), Ok((30, 30)));
    heap.collect(&mut |_| {});
    let new = heap
        .new_str(&format!("{}{}", "a".repeat(60), "\u{20ac}"))
        .unwrap();
    assert_eq!(new, old, "the address is reused");
    assert_eq!(heap.str_offset(new, 10).unwrap().0, 10);
}

#[test]
fn hash_is_fixed_and_spreads() {
    // Pinned: the hash is part of the snapshot/dict contract (spec §5).
    assert_eq!(hash_bytes(b""), 0);
    assert_eq!(hash_bytes(b"abc"), 0x9408_f1c6_2bed_b2f1);
    assert_ne!(hash_bytes(b"a"), hash_bytes(b"a\0"));
    // Low bits spread over a power-of-two table.
    let mut buckets = [0u32; 256];
    for i in 0..25_600u32 {
        let key = format!("name{i}");
        buckets[(hash_bytes(key.as_bytes()) & 255) as usize] += 1;
    }
    let max = buckets.iter().max().copied().unwrap();
    let min = buckets.iter().min().copied().unwrap();
    assert!(min > 50 && max < 160, "min {min} max {max}");
    // The compile-time twin agrees at every length around the 8-byte steps.
    let text: Vec<u8> = (0..40u8).map(|i| i.wrapping_mul(37) ^ 0xa5).collect();
    for n in 0..=text.len() {
        assert_eq!(hash_bytes_const(&text[..n]), hash_bytes(&text[..n]), "{n}");
    }
}

#[test]
fn interning_is_identity_and_survives_collections() {
    let mut heap = Heap::default();
    let a = heap.intern("spam").unwrap();
    assert_eq!(heap.intern("spam"), Ok(a));
    assert_ne!(heap.intern("eggs").unwrap(), a);
    assert_ne!(heap.header(a).unwrap().flags() & flags::INTERNED, 0);
    assert_eq!(heap.find_interned("spam"), Some(a));
    assert_eq!(heap.find_interned("ham"), None);
    // An existing str interns to the interned copy, or becomes it.
    let s = heap.new_str("spam").unwrap();
    assert_eq!(heap.intern_str(s), Ok(a));
    let h = heap.new_str("ham").unwrap();
    assert_eq!(heap.intern_str(h), Ok(h));
    assert_eq!(heap.intern("ham"), Ok(h));
    // Live interned strings keep their identities as the table grows.
    let names: Vec<String> = (0..5000).map(|i| format!("attr_{i}")).collect();
    let refs: Vec<Ref> = names.iter().map(|n| heap.intern(n).unwrap()).collect();
    assert_eq!(heap.interned_count(), 5003);
    for _ in 0..3 {
        heap.collect(&mut |t| {
            for &r in refs.iter().chain([a, h].iter()) {
                t.value(Value::from_ref(r, types::STR as u8));
            }
        });
        assert_eq!(heap.as_str(a).unwrap(), "spam");
        assert_eq!(heap.as_str(h).unwrap(), "ham");
        assert_eq!(heap.header(s), Err(Error::BadRef));
        for (n, &r) in names.iter().zip(&refs) {
            assert_eq!(heap.intern(n), Ok(r));
            assert_eq!(heap.as_str(r).unwrap(), n);
        }
        // Churn so freed memory is reused between collections.
        for i in 0..10_000 {
            heap.new_str(&format!("garbage {i}")).unwrap();
        }
    }
    assert_eq!(heap.interned_count(), 5002);
    assert_eq!(heap.find_interned("eggs"), None);
}

#[test]
fn weak_interning_preserves_wrapping_collision_chains() {
    let mut heap = Heap::default();
    // A cluster starts near the end of the initial table and wraps through 0.
    let names: Vec<String> = (0..)
        .map(|i| format!("collision{i}"))
        .filter(|s| hash_bytes(s.as_bytes()) & 63 == 62)
        .take(20)
        .collect();
    for round in 0..3 {
        let refs: Vec<Ref> = names.iter().map(|n| heap.intern(n).unwrap()).collect();
        heap.collect(&mut |t| {
            for (i, &r) in refs.iter().enumerate() {
                if (i + round) % 3 == 0 {
                    t.value(Value::from_ref(r, types::STR as u8));
                }
            }
        });
        assert_eq!(heap.interned_count(), if round == 1 { 6 } else { 7 });
        for (i, (name, &r)) in names.iter().zip(&refs).enumerate() {
            let live = (i + round) % 3 == 0;
            assert_eq!(heap.find_interned(name), live.then_some(r));
            if live {
                assert_eq!(heap.intern(name), Ok(r));
                assert_eq!(heap.as_str(r).unwrap(), name);
            } else {
                assert_eq!(heap.header(r), Err(Error::BadRef));
            }
        }
    }
    heap.collect(&mut |_| {});
    assert_eq!(heap.interned_count(), 0);
    for name in &names {
        assert_eq!(heap.find_interned(name), None);
        let r = heap.intern(name).unwrap();
        assert_eq!(heap.as_str(r).unwrap(), name);
    }
}

/// The first `n` names `<tag><k>` whose buckets in a table of `mask + 1`
/// slots satisfy `wanted`.
fn names_in_buckets(tag: &str, n: usize, mask: u64, wanted: impl Fn(u64) -> bool) -> Vec<String> {
    (0..)
        .map(|k| format!("{tag}{k}"))
        .filter(|s| wanted(hash_bytes(s.as_bytes()) & mask))
        .take(n)
        .collect()
}

/// Names for the buckets `buckets` (under `mask`), one each, in that
/// order: interned so, each lies at its bucket or right after the ones
/// that share it, however long the run of slots they fill.
fn names_at(tag: &str, mask: u64, buckets: &[u64]) -> Vec<String> {
    let mut wanted: Vec<Vec<usize>> = vec![Vec::new(); mask as usize + 1];
    for (k, &b) in buckets.iter().enumerate().rev() {
        wanted[(b & mask) as usize].push(k);
    }
    let mut names = vec![String::new(); buckets.len()];
    let mut left = buckets.len();
    for i in 0.. {
        if left == 0 {
            break;
        }
        let name = format!("{tag}{i}");
        if let Some(k) = wanted[(hash_bytes(name.as_bytes()) & mask) as usize].pop() {
            names[k] = name;
            left -= 1;
        }
    }
    names
}

/// The heap's intern table is one a checked restore admits: every entry
/// within `INTERN_WALK_CAP` slots of its hash's, with no free slot
/// between (`check_intern_table`).
fn assert_in_reach(heap: &Heap, what: &str) {
    let mut image = Vec::new();
    heap.write_snapshot(&mut image);
    let restored = Heap::read_snapshot(&mut mm_core::Reader::new(&image), u64::MAX);
    assert!(restored.is_ok(), "{what}: an entry out of a lookup's reach");
}

/// Collects `heap` with the names `keep` picks as roots, and checks that
/// the table then holds exactly those, each at its own string.
fn prune_to(heap: &mut Heap, names: &[String], refs: &[Ref], keep: impl Fn(usize) -> bool) {
    heap.collect(&mut |t| {
        for (i, &r) in refs.iter().enumerate() {
            if keep(i) {
                t.value(Value::from_ref(r, types::STR as u8));
            }
        }
    });
    let live = (0..names.len()).filter(|&i| keep(i)).count();
    assert_eq!(heap.interned_count() as usize, live);
    for (i, (name, &r)) in names.iter().zip(refs).enumerate() {
        assert_eq!(heap.find_interned(name), keep(i).then_some(r), "{name}");
    }
    assert_in_reach(heap, "pruned");
}

#[test]
fn a_long_cluster_prunes_to_its_live_names() {
    type Keep = (&'static str, fn(usize) -> bool);
    // 3,000 names fill one run of the 8,192 slots they end in, from its
    // start or across its end, two to every other bucket so none is far
    // from its own; the table doubles seven times on the way, over the
    // run.
    let run = |first: u64| -> Vec<u64> { (0..3000).map(|k| first + (k & !1)).collect() };
    let from_start = names_at("low", 8191, &run(0));
    let across_end = names_at("high", 8191, &run(8192 - 1400));
    let keeps: [Keep; 7] = [
        ("none", |_| false),
        ("all", |_| true),
        ("every second", |i| i % 2 == 0),
        ("one in seven", |i| i % 7 == 3),
        ("all but one in seven", |i| i % 7 != 3),
        ("the first", |i| i == 0),
        ("the last third", |i| i >= 2000),
    ];
    for names in [&from_start, &across_end] {
        for (what, keep) in keeps {
            let mut heap = Heap::default();
            let refs: Vec<Ref> = names.iter().map(|n| heap.intern(n).unwrap()).collect();
            for (name, &r) in names.iter().zip(&refs) {
                assert_eq!(heap.find_interned(name), Some(r), "{what}: {name}");
            }
            let slots = intern_slots(&heap);
            assert_eq!(slots.len(), 8192, "no doubling for room near a bucket");
            let longest = slots.split(Option::is_none).map(<[_]>::len).max();
            // (The run across the end reads as two.)
            assert!(longest >= Some(1400), "{longest:?}");
            prune_to(&mut heap, names, &refs, keep);
            // The table takes the dead names again, and loses all of them.
            let again: Vec<Ref> = names.iter().map(|n| heap.intern(n).unwrap()).collect();
            for (i, (&r, &old)) in again.iter().zip(&refs).enumerate() {
                // (A dead name's new string may lie where the old one did.)
                assert!(r == old || !keep(i), "{what}: {}", names[i]);
                assert_eq!(heap.as_str(r).unwrap(), names[i]);
            }
            assert_eq!(heap.interned_count(), 3000);
            prune_to(&mut heap, names, &again, |_| false);
        }
    }
}

#[test]
fn a_table_of_long_clusters_grows_with_every_name_in_reach() {
    // Names of consecutive buckets, of the table they end in and of every
    // smaller one: runs of thousands at each doubling, one across the
    // table's end, after a few names of any bucket (few: each puts the
    // names of a run it lies in one slot further from their buckets).
    let run = |first: u64, n: u64| -> Vec<u64> { (0..n).map(|k| first + k).collect() };
    let mut names: Vec<String> = (0..100).map(|i| format!("any{i}")).collect();
    names.extend(names_at("a", 16383, &run(2000, 5000)));
    names.extend(names_at("b", 16383, &run(15_500, 1500)));
    let mut heap = Heap::default();
    let mut refs = Vec::new();
    for (i, name) in names.iter().enumerate() {
        refs.push(heap.intern(name).unwrap());
        // Around each doubling, every name so far is where a lookup looks.
        if (i + 2).is_power_of_two() || (i + 1).is_power_of_two() {
            for (name, &r) in names.iter().zip(&refs) {
                assert_eq!(heap.find_interned(name), Some(r), "after {i}: {name}");
            }
        }
    }
    assert_eq!(heap.interned_count(), 6600);
    assert_eq!(intern_slots(&heap).len(), 16_384);
    assert_in_reach(&heap, "grown");
    // A batch reserved late doubles the table more than once.
    heap.reserve_interned(60_000).unwrap();
    assert_eq!(intern_slots(&heap).len(), 131_072);
    // A first table is made at the batch's size.
    let mut fresh = Heap::default();
    fresh.reserve_interned(1000).unwrap();
    assert_eq!(intern_slots(&fresh).len(), 2048);
    for (name, &r) in names.iter().zip(&refs) {
        assert_eq!(heap.find_interned(name), Some(r), "{name}");
        assert_eq!(heap.intern(name), Ok(r));
    }
    assert_eq!(heap.interned_count(), 6600);
    assert_eq!(heap.find_interned("absent"), None);
    prune_to(&mut heap, &names, &refs, |i| i % 3 == 0);
}

/// The heap's intern table (the largest: grown ones are garbage until a
/// collection) as its slots' references.
fn intern_slots(heap: &Heap) -> Vec<Option<Ref>> {
    let mut table = None;
    heap.for_each_object(&mut |r, h| {
        if h.type_id() == types::INTERN_TABLE && table.is_none_or(|(_, len)| h.len() > len) {
            table = Some((r, h.len()));
        }
    });
    let (table, len) = table.unwrap();
    (1..len)
        .map(|i| heap.get(table, i).unwrap().as_obj())
        .collect()
}

/// What a prune must leave of `slots` (each entry with its hash), in a
/// model written apart from the heap's: every cluster that holds a dead
/// entry laid out again, its survivors by bucket and then reference, each
/// at its bucket or right after the one before it.
fn model_prune(slots: &mut [Option<(Ref, u64)>], live: &dyn Fn(Ref) -> bool) {
    let cap = slots.len();
    let mask = cap - 1;
    for i in 0..cap {
        if slots[i].is_none_or(|(r, _)| live(r)) {
            continue;
        }
        let mut base = i;
        while slots[base.wrapping_sub(1) & mask].is_some() {
            base = base.wrapping_sub(1) & mask;
        }
        let len = (0..cap)
            .find(|k| slots[(base + k) & mask].is_none())
            .unwrap();
        let mut kept: Vec<(usize, Ref, u64)> = (0..len)
            .filter_map(|k| slots[(base + k) & mask].take())
            .filter(|&(r, _)| live(r))
            .map(|(r, hash)| ((hash as usize).wrapping_sub(base) & mask, r, hash))
            .collect();
        kept.sort_unstable_by_key(|&(distance, r, hash)| (distance, hash_tag(hash), r.raw()));
        let mut next = 0;
        for (distance, r, hash) in kept {
            let at = next.max(distance);
            slots[(base + at) & mask] = Some((r, hash));
            next = at + 1;
        }
    }
}

/// A snapshot holds the table's slots as they lie, so where a prune and a
/// doubling leave each entry is part of what an image is (`SNAPSHOT_EPOCH`
/// moves with it).
#[test]
fn pruned_and_doubled_tables_have_the_slots_of_the_model() {
    let short: Vec<String> = (0..900).map(|i| format!("name{i}")).collect();
    let mut mixed = short.clone();
    let run = |first: u64, n: u64| -> Vec<u64> { (0..n).map(|k| first + (k & !1)).collect() };
    mixed.extend(names_at("m", 4095, &run(500, 300)));
    mixed.extend(names_at("e", 4095, &run(4000, 200)));
    // A long cluster after short ones with dead names.
    mixed.extend(names_at("c", 4095, &run(2000, 300)));
    for (what, names, longest) in [("short", &short, 0..=64), ("mixed", &mixed, 200..=4096)] {
        for keep in [
            |i: usize| i.is_multiple_of(2),
            |i: usize| i % 5 != 1,
            |_| false,
        ] {
            let mut heap = Heap::default();
            let refs: Vec<Ref> = names.iter().map(|n| heap.intern(n).unwrap()).collect();
            let hash_of = |r: Ref| {
                let i = refs.iter().position(|&x| x == r).unwrap();
                hash_bytes(names[i].as_bytes())
            };
            let before = intern_slots(&heap);
            // No doubling for room near a bucket.
            let cap = (names.len() * 2 + 2).next_power_of_two();
            assert_eq!(before.len(), cap, "{what}");
            let run = before.split(Option::is_none).map(<[_]>::len).max().unwrap();
            assert!(longest.contains(&run), "{what}: longest cluster {run}");
            let mut model: Vec<Option<(Ref, u64)>> =
                before.iter().map(|s| s.map(|r| (r, hash_of(r)))).collect();
            let live = |r: Ref| keep(refs.iter().position(|&x| x == r).unwrap());
            model_prune(&mut model, &live);
            prune_to(&mut heap, names, &refs, keep);
            let want: Vec<Option<Ref>> = model.iter().map(|s| s.map(|(r, _)| r)).collect();
            assert_eq!(intern_slots(&heap), want, "{what}: pruned");
        }
    }
    // Doubling: the clusters in table order from the slot after the first
    // free one, each sorted by where its entries' buckets lie after that
    // slot in the new table; each entry then at its new bucket's first
    // free slot.
    for names in [&short, &mixed] {
        doubled_as_the_model(names);
    }
}

fn doubled_as_the_model(mixed: &[String]) {
    let mut heap = Heap::default();
    let refs: Vec<Ref> = mixed.iter().map(|n| heap.intern(n).unwrap()).collect();
    let hash_of = |r: Ref| {
        let i = refs.iter().position(|&x| x == r).unwrap();
        hash_bytes(mixed[i].as_bytes()) as usize
    };
    let before = intern_slots(&heap);
    let cap = before.len();
    heap.reserve_interned(u32::try_from(cap).unwrap()).unwrap();
    let origin = before.iter().position(Option::is_none).unwrap() + 1;
    let mut order: Vec<Ref> = Vec::new();
    let mut cluster: Vec<Ref> = Vec::new();
    for k in 0..=cap {
        if let Some(r) = before[(origin + k) % cap].filter(|_| k < cap) {
            cluster.push(r);
            continue;
        }
        // By the half of the new table the entry goes to and its bucket
        // there, counted from the origin; then hash byte and reference.
        cluster.sort_by_key(|&r| {
            let hash = hash_of(r);
            let key = hash.wrapping_sub(origin) & (2 * cap - 1);
            (key, hash_tag(hash as u64), r.raw())
        });
        order.append(&mut cluster);
    }
    let mut want = vec![None; cap * 2];
    for r in order {
        let mut at = hash_of(r) & (2 * cap - 1);
        while want[at].is_some() {
            at = (at + 1) & (2 * cap - 1);
        }
        want[at] = Some(r);
    }
    assert_eq!(intern_slots(&heap), want, "doubled");
}

/// `n` names whose hashes agree in the bits of `mask`: they share a
/// bucket in every table of at most `mask + 1` slots.
fn names_of_one_bucket(tag: &str, n: usize, mask: u64) -> Vec<String> {
    names_in_buckets(tag, n, mask, |b| b == 7)
}

#[test]
fn an_insert_with_no_slot_near_its_bucket_doubles_the_table() {
    // 140 names of one bucket of every table up to 4,096 slots: the
    // table holds 129 of them (walks of 0 to the cap), and the next one
    // doubles it until they part, far past the size their count asks.
    let names = names_of_one_bucket("crowd", 140, 4095);
    let mut heap = Heap::default();
    let mut refs = Vec::new();
    for (i, name) in names.iter().enumerate() {
        refs.push(heap.intern(name).unwrap());
        let cap = intern_slots(&heap).len();
        if i <= INTERN_WALK_CAP {
            assert!(cap <= 512, "{i}: {cap} slots");
            let slots = intern_slots(&heap);
            let longest = slots.split(Option::is_none).map(<[_]>::len).max();
            assert!(longest >= Some(i + 1), "{i}");
        } else {
            // In one growth, to the first size at which they part.
            assert_eq!(cap, 8192, "{i}");
        }
        assert_in_reach(&heap, name);
        // Found again at once, the one at the cap too.
        assert_eq!(heap.find_interned(name), refs.last().copied(), "{i}");
        assert_eq!(heap.interned_count() as usize, i + 1);
    }
    for (name, &r) in names.iter().zip(&refs) {
        assert_eq!(heap.find_interned(name), Some(r), "{name}");
        assert_eq!(heap.intern(name), Ok(r));
    }
    assert_eq!(heap.interned_count(), 140);
    prune_to(&mut heap, &names, &refs, |i| i % 2 == 0);
}

#[test]
fn names_crowded_at_the_largest_table_are_out_of_memory() {
    // The largest table an object may be, and names of 64 of its buckets
    // until one finds no slot within the cap.
    let mut heap = Heap::default();
    heap.reserve_interned(2_000_000).unwrap();
    assert_eq!(intern_slots(&heap).len(), 1 << 22);
    let mask = (1 << 22) - 1;
    let mut crowd = (0..)
        .map(|k| format!("z{k}"))
        .filter(|s| hash_bytes(s.as_bytes()) & mask < 64);
    let mut held = Vec::new();
    let refused = loop {
        let name = crowd.next().unwrap();
        match heap.intern(&name) {
            Ok(r) => held.push((name, r)),
            Err(e) => break (name, e),
        }
        assert!(held.len() < 64 + INTERN_WALK_CAP + 1, "never refused");
    };
    assert_eq!(refused.1, Error::OutOfMemory);
    assert!(held.len() > INTERN_WALK_CAP, "{} names", held.len());
    // Nothing of the refused name is left, as a str either.
    assert_eq!(heap.find_interned(&refused.0), None);
    assert_eq!(heap.interned_count() as usize, held.len());
    let plain = heap.new_str(&refused.0).unwrap();
    assert_eq!(heap.intern_str(plain), Err(Error::OutOfMemory));
    assert_eq!(heap.header(plain).unwrap().flags() & flags::INTERNED, 0);
    assert_eq!(intern_slots(&heap).len(), 1 << 22);
    // Refused before another table was made.
    let mut tables = 0;
    heap.for_each_object(&mut |_, h| tables += usize::from(h.type_id() == types::INTERN_TABLE));
    assert_eq!(tables, 1);
    // Every other name interns as before.
    for (name, r) in &held {
        assert_eq!(heap.intern(name), Ok(*r));
    }
    let apart = heap.intern("a name of another bucket").unwrap();
    assert_eq!(heap.find_interned("a name of another bucket"), Some(apart));
    assert_in_reach(&heap, "crowded");
    // Once the crowd is dropped, the name has room.
    heap.collect(&mut |t| t.value(Value::from_ref(apart, types::STR as u8)));
    assert_eq!(heap.interned_count(), 1);
    let again = heap.intern(&refused.0).unwrap();
    assert_eq!(heap.find_interned(&refused.0), Some(again));
}

/// Whatever is interned, pruned and doubled, no entry is left further
/// from its hash's slot than the cap, or behind a free slot: names of
/// any bucket, runs of consecutive buckets, names crowded on a few
/// buckets (walks near the cap) and on one (which doubles the table).
#[test]
fn no_prune_or_doubling_leaves_an_entry_past_the_walk_cap() {
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    for round in 0..12_u64 {
        let mut heap = Heap::default();
        let mut names: Vec<String> = Vec::new();
        let mut refs: Vec<Ref> = Vec::new();
        for step in 0..10_u64 {
            let tag = format!("r{round}s{step}_");
            let mask = (64_u64 << next(7)) - 1;
            let first = next(mask + 1);
            let batch = match next(4) {
                0 => (0..40 + next(300)).map(|i| format!("{tag}{i}")).collect(),
                1 => {
                    let run: Vec<u64> = (0..20 + next(200)).map(|k| first + k / 2).collect();
                    names_at(&tag, mask, &run)
                }
                2 => names_in_buckets(&tag, 60 + next(60) as usize, mask, |b| {
                    b.wrapping_sub(first) & mask < 8
                }),
                _ => names_in_buckets(&tag, 100 + next(60) as usize, 1023, |b| b == first & 1023),
            };
            for name in batch {
                refs.push(heap.intern(&name).unwrap());
                names.push(name);
            }
            assert_in_reach(&heap, &format!("{tag}interned"));
            if next(3) == 0 {
                let cap = u32::try_from(intern_slots(&heap).len()).unwrap();
                heap.reserve_interned(cap).unwrap();
                assert_in_reach(&heap, &format!("{tag}doubled"));
            }
            let (period, phase) = (2 + next(5), next(2));
            let keep: Vec<bool> = (0..names.len())
                .map(|i| !(i as u64 + phase).is_multiple_of(period))
                .collect();
            prune_to(&mut heap, &names, &refs, |i| keep[i]);
            let mut kept = keep.iter();
            names.retain(|_| *kept.next().unwrap());
            let mut kept = keep.iter();
            refs.retain(|_| *kept.next().unwrap());
        }
    }
}

#[test]
fn scratch_owners_share_the_budget_and_release_independently() {
    let mut heap = Heap::new(HeapConfig {
        memory_limit: 4096,
        ..HeapConfig::default()
    });
    let mut first = heap.reserve_scratch(123).unwrap();
    let available = heap.available_bytes();
    let second = heap.reserve_scratch(517).unwrap();
    assert_eq!(heap.available_bytes(), available - 517);
    heap.set_external_bytes(available - 517).unwrap();
    assert_eq!(heap.available_bytes(), 0);
    assert_eq!(first.resize(&heap, 124), Err(Error::OutOfMemory));
    assert_eq!(heap.available_bytes(), 0);
    assert_eq!(heap.alloc_bytes_zeroed(1), Err(Error::OutOfMemory));
    // Dropping on another thread preserves Heap's Send/Sync contract.
    std::thread::spawn(move || drop(second)).join().unwrap();
    assert_eq!(heap.available_bytes(), 517);
    first.resize(&heap, 640).unwrap();
    assert_eq!(heap.available_bytes(), 0);
    let other = Heap::default();
    assert_eq!(first.resize(&other, 0), Err(Error::BadRef));
    assert_eq!(heap.available_bytes(), 0);
    first.resize(&heap, 17).unwrap();
    assert_eq!(heap.available_bytes(), 623);
    drop(first);
    assert_eq!(heap.available_bytes(), 640);
    heap.set_external_bytes(0).unwrap();
    assert_eq!(heap.available_bytes(), available + 123);
    let last = heap.reserve_scratch(11).unwrap();
    drop(heap);
    drop(last);

    // The shared counter's 24 bytes are charged from the start.
    let tiny = Heap::new(HeapConfig {
        memory_limit: 31,
        ..HeapConfig::default()
    });
    assert_eq!(tiny.available_bytes(), 7);
    assert!(matches!(tiny.reserve_scratch(8), Err(Error::OutOfMemory)));
    assert_eq!(tiny.available_bytes(), 7);
}

#[test]
fn shared_scratch_growth_cannot_overcommit() {
    let mut heap = Heap::new(HeapConfig {
        memory_limit: 4 * SEGMENT_BYTES,
        ..HeapConfig::default()
    });
    // Include both arena spaces, their metadata and collector workspace in
    // the fixed charges. Scratch must not compete against the total limit.
    heap.alloc_bytes_zeroed(47).unwrap();
    heap.alloc(T, 0, 3, 0).unwrap();
    let owners: Vec<_> = (0..8).map(|_| heap.reserve_scratch(0).unwrap()).collect();
    heap.set_external_bytes(heap.available_bytes() - 1031)
        .unwrap();
    let heap = &heap;
    let barrier = std::sync::Barrier::new(9);
    let mut observed = Vec::new();
    std::thread::scope(|scope| {
        for mut owner in owners {
            let barrier = &barrier;
            scope.spawn(move || {
                for _ in 0..128 {
                    barrier.wait();
                    let _ = owner.resize(heap, 263);
                    barrier.wait();
                    // Hold successful charges until the main thread checks.
                    barrier.wait();
                    let _ = owner.resize(heap, 0);
                    barrier.wait();
                }
            });
        }
        for _ in 0..128 {
            barrier.wait();
            barrier.wait();
            let remaining = heap.available_bytes();
            barrier.wait();
            barrier.wait();
            observed.push((remaining, heap.available_bytes()));
        }
    });
    // Assert after all barriers so a regression fails rather than deadlocking.
    assert_eq!(observed, vec![(1031 - 3 * 263, 1031); 128]);
}

#[test]
fn would_fit_is_exact_for_large_sizes() {
    let mut heap = Heap::new(HeapConfig {
        memory_limit: 10 * SEGMENT_BYTES,
        ..HeapConfig::default()
    });
    assert!(heap.would_fit(9 * SEGMENT_BYTES));
    assert!(!heap.would_fit(10 * SEGMENT_BYTES));
    // Rounding, the object header and the large-block overhead can each
    // overflow independently. Checking only MAX misses the last one.
    for bytes in u64::MAX - 32..=u64::MAX {
        assert!(!heap.would_fit(bytes), "{bytes}");
    }
    heap.set_external_bytes(2 * SEGMENT_BYTES).unwrap();
    assert!(!heap.would_fit(9 * SEGMENT_BYTES));
    assert_eq!(
        heap.alloc_bytes_zeroed(9 * SEGMENT_BYTES as usize),
        Err(Error::OutOfMemory)
    );
    assert_eq!(
        heap.set_external_bytes(11 * SEGMENT_BYTES),
        Err(Error::OutOfMemory)
    );
    assert!(!heap.would_fit(9 * SEGMENT_BYTES));
    assert!(heap.would_fit(7 * SEGMENT_BYTES));
    heap.set_external_bytes(0).unwrap();
    heap.alloc_bytes_zeroed(9 * SEGMENT_BYTES as usize).unwrap();
    assert!(!heap.would_fit(SEGMENT_BYTES));
    assert_eq!(
        heap.alloc_bytes_zeroed(SEGMENT_BYTES as usize),
        Err(Error::OutOfMemory)
    );
}

#[test]
fn appended_bytes_grow_in_place_within_the_limit_and_the_cap() {
    let mut heap = Heap::new(HeapConfig {
        memory_limit: 8 << 20,
        ..HeapConfig::default()
    });
    // A small block is copied; once large, it keeps its address and room.
    let mut b = heap.alloc_bytes(b"ab").unwrap();
    b = heap.append_bytes(b, b"c", 0).unwrap();
    assert_eq!(heap.bytes(b).unwrap(), b"abc");
    let chunk = vec![b'x'; 200 * 1024];
    b = heap.append_bytes(b, &chunk, 1 << 20).unwrap();
    let (at, committed) = (b, heap.committed_bytes());
    b = heap.append_bytes(b, &chunk, 1 << 20).unwrap();
    assert_eq!((b, heap.committed_bytes()), (at, committed));
    let len = 3 + 400 * 1024;
    assert_eq!(heap.bytes(b).unwrap().len(), len);
    // Room past the limit or the largest object is refused before anything
    // is written.
    assert_eq!(
        heap.append_bytes(b, b"y", 16 << 20),
        Err(Error::OutOfMemory)
    );
    let past = usize::try_from(mm_core::MAX_OBJECT_BYTES).unwrap() + 1;
    assert_eq!(heap.append_bytes(b, b"y", past), Err(Error::OutOfMemory));
    assert_eq!(heap.bytes(b).unwrap().len(), len);
    // No data and no reserve give the spare room back; the str takes the
    // block as it is.
    b = heap.append_bytes(b, &[], 0).unwrap();
    assert_eq!(b, at);
    assert!(heap.committed_bytes() + 512 * 1024 < committed);
    let s = heap.str_from_block(b, len as u64).unwrap();
    assert_eq!(heap.str_bytes(s).unwrap().len(), len);
    assert_eq!(&heap.str_bytes(s).unwrap()[..4], b"abcx");
    // Growing past the buffer may move it, holding the old buffer and the
    // new one at once: room whose growth alone fits is refused when both
    // do not.
    let mut c = heap.alloc_bytes(&[]).unwrap();
    c = heap.append_bytes(c, &vec![b'z'; 3 << 20], 3 << 20).unwrap();
    let free = usize::try_from(heap.available_bytes()).unwrap();
    assert!(free > 4 << 20, "{free}");
    let both = (3 << 20) + free - (1 << 20);
    assert_eq!(heap.append_bytes(c, b"z", both), Err(Error::OutOfMemory));
    assert!(heap.append_bytes(c, b"z", free - (64 << 10)).is_ok());
}
