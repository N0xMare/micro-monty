# str case predicates, case mappings and isidentifier follow Unicode 16.0
# (CPython 3.14's), not the newer tables of the Rust toolchain: U+0295 is
# Ll in 16 (Lo in 17); U+A7CE..U+A7D5, U+A7F1 and U+16EA0.. are unassigned
# in 16, as are the letters Unicode 18 adds (Rust 1.100's tables), and
# U+0277, U+027C, U+AB4B and U+AB4C gain an uppercase there; U+FE71,
# U+FE73, ... are identifier characters. Found by difffuzz (seed 30764285:
# chr(661).islower()).
import zlib

c = chr(0x295)
print(c.islower(), c.isupper(), c.istitle(), ascii(c.upper()), ascii(c.swapcase()))
print(("a" + c).islower(), (c + "B").istitle(), ascii(("x" + c + "y").title()))

new = "\ua7ce\ua7cf\ua7d2\ua7d3\ua7d4\ua7d5\ua7f1\U00016ea0\U00016ebb"
print(ascii(new.upper()), ascii(new.lower()), ascii(new.casefold()))
print(ascii(new.title()), ascii(new.swapcase()), ascii(new.capitalize()))
print([ch.islower() for ch in new], [ch.isupper() for ch in new])
print(("Ab" + new).istitle(), (new + "a").islower(), (new + "A").isupper())

new18 = "\u0277\u027c\uab4b\uab4c\u0558\u058b\u209d\ua7dd\ua7e2\uab6c\U000107bb\U0001d6a6\U0001df1f\U0001df40\U0001df90\U0001dfcd"
print(ascii(new18.upper()), ascii(new18.lower()), ascii(new18.casefold()))
print(ascii(new18.title()), ascii(new18.swapcase()), ascii(new18.capitalize()))
print([ch.islower() for ch in new18], [ch.isupper() for ch in new18])

print([hex(cp) for cp in range(0xFE70, 0xFE80) if chr(cp).isidentifier()])
print([hex(cp) for cp in range(0xFE70, 0xFE80) if ("a" + chr(cp)).isidentifier()])

# Every predicate and mapping over the blocks around them.
blocks = [range(0x250, 0x2B0), range(0xA720, 0xA800), range(0xFE70, 0xFF00),
          range(0x16EA0, 0x16EE0), range(0x1C80, 0x1C90), range(0x10D50, 0x10D90),
          range(0x550, 0x590), range(0x2090, 0x20A0), range(0xAB30, 0xAB70),
          range(0x107B0, 0x107C0), range(0x1D6A0, 0x1D6B0), range(0x1DF00, 0x1E000)]
out = []
for block in blocks:
    for cp in block:
        ch = chr(cp)
        out.append((cp, ch.islower(), ch.isupper(), ch.istitle(), ch.isalpha(),
                    ch.isidentifier(), ch.lower(), ch.upper(), ch.title(),
                    ch.casefold(), ch.swapcase()))
print(len(out), zlib.crc32(ascii(out).encode()))
