"""Generates `src/ucd/*.bin` and `src/ucd/tables.rs`, the Unicode Character
Database tables behind mm-engine's `unicodedata` feature, from the running
CPython's `unicodedata` (3.14: Unicode 16.0.0).

Run from crates/mm-engine: `python3.14 tools/gen_ucd.py`.

Formats (all little-endian; read by src/unicodedata.rs):

- `cat.bin`: u32 per range, `start << 5 | category index` (CATEGORIES), sorted;
  a range runs to the next start.
- `ccc.bin`: u32 per range, `start << 8 | canonical combining class`.
- `num.bin`: u32 per character with a numeric value that is not a decimal
  digit, `cp << 11 | value index`, sorted; `numval.bin`: the f64 values.
- `dkey.bin`: u32 per decomposition (Hangul syllables are algorithmic),
  `cp | len << 21 | tag << 26` (tag 0 canonical, else TAGS[tag - 1]),
  sorted; `doff.bin`: u16 offset of each into `ddata.bin`, the decompositions
  in UTF-8.
- `comp.bin`: u32 triples (first, second, composite) of the primary
  composites, sorted by (first, second).
- `lex.bin`: the words of the character names, most frequent first, the last
  byte of each with bit 7 set. `names.bin`: one record per named character
  in code point order (algorithmic names excluded): the code point delta
  (LEB128), a byte `shared << 4 | count` (count 15: a second byte holds it):
  the first `shared` words repeat the previous name's, then `count` word
  codes, one byte below 192, else two: `192 + (id - 192) / 256`, `(id - 192) % 256`.
- `printable.bin` (used without the `unicodedata` feature, for `repr` and
  `str.isprintable`): u16 per plane 0..=17, the index of the plane's first
  boundary, then u16 per boundary, the low 16 bits of each code point where
  `str.isprintable` changes, in order from U+0000 (not printable). A code
  point with an even number of boundaries at or below it is printable.
- tables.rs: CATEGORIES, TAGS and ALGORITHMIC, the `PREFIX-%X` name ranges.

`unicodedata.py` (mm-stdlib) holds `unidata_version`.
"""

import collections
import re
import struct
import sys
import unicodedata as u

OUT = 'src/ucd/'
MAXCP = sys.maxunicode + 1

CATEGORIES = ['Cn', 'Lu', 'Ll', 'Lt', 'Lm', 'Lo', 'Mn', 'Mc', 'Me', 'Nd', 'Nl', 'No', 'Pc', 'Pd',
              'Ps', 'Pe', 'Pi', 'Pf', 'Po', 'Sm', 'Sc', 'Sk', 'So', 'Zs', 'Zl', 'Zp', 'Cc', 'Cf',
              'Cs', 'Co']


def write(name, data):
    with open(OUT + name, 'wb') as f:
        f.write(data)


def ranges(value, shift):
    out = []
    prev = None
    for c in range(MAXCP):
        v = value(c)
        if v != prev:
            out.append(c << shift | v)
            prev = v
    return struct.pack(f'<{len(out)}I', *out)


def is_hangul(c):
    return 0xAC00 <= c <= 0xD7A3


def main():
    write('cat.bin', ranges(lambda c: CATEGORIES.index(u.category(chr(c))), 5))
    write('ccc.bin', ranges(lambda c: u.combining(chr(c)), 8))

    bounds = []
    prev = False
    for c in range(MAXCP):
        p = chr(c).isprintable()
        if p != prev or c == 0:
            bounds.append(c)
            prev = p
    planes = [sum(1 for b in bounds if b >> 16 < k) for k in range(18)]
    write('printable.bin', struct.pack(f'<18H{len(bounds)}H', *planes, *(b & 0xFFFF for b in bounds)))

    values = []
    nums = []
    for c in range(MAXCP):
        v = u.numeric(chr(c), None)
        if v is None or chr(c).isdecimal():
            continue
        if v not in values:
            values.append(v)
        nums.append(c << 11 | values.index(v))
    assert len(values) < 2048
    write('num.bin', struct.pack(f'<{len(nums)}I', *nums))
    write('numval.bin', struct.pack(f'<{len(values)}d', *values))

    tags = []
    keys = []
    offs = []
    data = bytearray()
    for c in range(MAXCP):
        d = u.decomposition(chr(c))
        if not d or is_hangul(c):
            continue
        parts = d.split()
        tag = 0
        if parts[0].startswith('<'):
            if parts[0] not in tags:
                tags.append(parts[0])
            tag = tags.index(parts[0]) + 1
            parts = parts[1:]
        assert len(parts) < 32 and tag < 32
        keys.append(c | len(parts) << 21 | tag << 26)
        offs.append(len(data))
        data += ''.join(chr(int(p, 16)) for p in parts).encode()
    assert len(data) < 65536
    write('dkey.bin', struct.pack(f'<{len(keys)}I', *keys))
    write('doff.bin', struct.pack(f'<{len(offs)}H', *offs))
    write('ddata.bin', bytes(data))

    comp = []
    for c in range(MAXCP):
        d = u.decomposition(chr(c))
        if not d or d.startswith('<') or is_hangul(c):
            continue
        parts = [int(p, 16) for p in d.split()]
        if len(parts) == 2 and u.normalize('NFC', chr(parts[0]) + chr(parts[1])) == chr(c):
            comp.append((parts[0], parts[1], c))
    comp.sort()
    write('comp.bin', b''.join(struct.pack('<3I', *t) for t in comp))

    algorithmic = []
    names = []
    for c in range(MAXCP):
        n = u.name(chr(c), None)
        if n is None or is_hangul(c):
            continue
        m = re.fullmatch(r'(.*)-([0-9A-F]{4,6})', n)
        if m and int(m.group(2), 16) == c:
            if algorithmic and algorithmic[-1][2] == m.group(1) and algorithmic[-1][1] == c - 1:
                algorithmic[-1][1] = c
            else:
                algorithmic.append([c, c, m.group(1)])
            continue
        names.append((c, n.split(' ')))
    words = collections.Counter(w for _, ws in names for w in ws)
    ranked = [w for w, _ in words.most_common()]
    assert len(ranked) < 192 + 64 * 256
    ids = {w: i for i, w in enumerate(ranked)}
    lex = bytearray()
    for w in ranked:
        b = bytearray(w.encode('ascii'))
        b[-1] |= 0x80
        lex += b
    write('lex.bin', bytes(lex))
    rec = bytearray()
    prev_c = -1
    prev_ws = []
    for c, ws in names:
        delta = c - prev_c
        while True:
            b = delta & 0x7F
            delta >>= 7
            rec.append(b | (0x80 if delta else 0))
            if not delta:
                break
        k = 0
        while k < min(len(ws), len(prev_ws), 15) and ws[k] == prev_ws[k]:
            k += 1
        m = len(ws) - k
        if m >= 15:
            rec += bytes([k << 4 | 15, m])
        else:
            rec.append(k << 4 | m)
        for w in ws[k:]:
            i = ids[w]
            if i < 192:
                rec.append(i)
            else:
                rec += bytes([192 + (i - 192) // 256, (i - 192) % 256])
        prev_c, prev_ws = c, ws
    write('names.bin', bytes(rec))

    with open(OUT + 'tables.rs', 'w') as f:
        f.write('// Generated by tools/gen_ucd.py from CPython ' + sys.version.split()[0]
                + "'s unicodedata (Unicode " + u.unidata_version + '); do not edit.\n\n')
        f.write('pub(crate) const CATEGORIES: [&str; 30] = [\n')
        f.write(''.join(f'    "{c}",\n' for c in CATEGORIES))
        f.write('];\n\n')
        f.write(f'pub(crate) const TAGS: [&str; {len(tags)}] = [\n')
        f.write(''.join(f'    "{t}",\n' for t in tags))
        f.write('];\n\n')
        f.write('/// `PREFIX-%X` names: (first, last, prefix).\n')
        f.write(f'pub(crate) const ALGORITHMIC: [(u32, u32, &str); {len(algorithmic)}] = [\n')
        f.write(''.join(f'    (0x{a:X}, 0x{b:X}, "{p}"),\n' for a, b, p in algorithmic))
        f.write('];\n')


main()
