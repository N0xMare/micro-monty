# repr escapes exactly the code points str.isprintable rejects: unassigned
# (Cn), format, separator, control, surrogate and private-use characters.
print(repr(chr(0x378)))
samples = [0x378, 0x1aec, 0xad, 0x85, 0xa0, 0x2028, 0x200b, 0x2066, 0xfeff, 0xfffe,
           0xe000, 0x10fffd, 0xf0000, 0x1f600, 0x1f8b0, 0x1fbff, 0x2a6e0, 0x2a6df,
           0x323af, 0x323b0, 0xe0001, 0xe0100, 0xe01f0, 0x300, 0x3000, 0x1680, 0x30000]
for cp in samples:
    c = chr(cp)
    print(hex(cp), repr(c), ascii(c), c.isprintable(), f"{c!r}", "%r" % c)
print([chr(0x378), 'é', chr(0xe0100)], {chr(0x1aec): chr(0x2fffe)})
for f in (int, float):
    try:
        f(chr(0x1aec))
    except ValueError as e:
        print(e)
try:
    {}[chr(0x10ffff)]
except KeyError as e:
    print(e)

# Every 13th code point, per plane: how many repr escapes, and whether repr
# and isprintable agree on each (surrogates aside: micro-monty strings are
# UTF-8). mm-engine's unit tests check every code point.
for plane in range(17):
    escaped = disagree = 0
    for cp in range(plane << 16, (plane + 1) << 16, 13):
        if 0xD800 <= cp < 0xE000:
            continue
        c = chr(cp)
        r = repr(c)
        plain = r == "'" + c + "'" or c in "'\\"
        escaped += not plain
        disagree += plain != c.isprintable()
    print(plane, escaped, disagree)
