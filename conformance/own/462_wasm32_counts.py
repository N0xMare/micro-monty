# Counts, lengths, widths and precisions past 2 ** 32 behave as on a 64-bit
# CPython on every target (sys.maxsize is 64-bit in micro-monty, wasm32
# included). On wasm32 these went through a 32-bit usize: b'ab'.center(2**62)
# was b'ab', len(range(2**40)) an OverflowError, range(2**40)[2**33] an
# IndexError, format('abc', '.4294967296') a ValueError and
# struct.calcsize('8589934592x') a struct.error. Slices of a range keep the
# slice's stop, as CPython's compute_slice does.
import struct


def show(label, f):
    try:
        print(label, repr(f()))
    except (OverflowError, MemoryError, ValueError, IndexError, struct.error) as e:
        print(label, type(e).__name__, e)


B = 2**40
K = 2**32 + 1
show("len(range(B))", lambda: len(range(B)))
show("len(range(0, 2 ** 33, 2))", lambda: len(range(0, 2**33, 2)))
show("len(range(0, B, K))", lambda: len(range(0, B, K)))
show("range(B)[-1]", lambda: range(B)[-1])
show("range(B)[-K]", lambda: range(B)[-K])
show("range(B)[2 ** 33]", lambda: range(B)[2**33])
show("range(B)[B]", lambda: range(B)[B])
show("range(B)[2 ** 33:2 ** 33 + 3]", lambda: range(B)[2**33 : 2**33 + 3])
show("range(B)[::K]", lambda: range(B)[::K])
show("range(B)[-3:]", lambda: range(B)[-3:])
show("len(range(B)[K:])", lambda: len(range(B)[K:]))
show("range(10)[::3]", lambda: range(10)[::3])
show("range(1, 20, 3)[5:1:-2]", lambda: range(1, 20, 3)[5:1:-2])
show("range(10)[100:]", lambda: range(10)[100:])
show("next(reversed(range(B)))", lambda: next(reversed(range(B))))
show("len(range(-2 ** 63, 2 ** 63 - 1))", lambda: len(range(-(2**63), 2**63 - 1)))
show("list(range(2 ** 63 - 3, 2 ** 63 - 1))", lambda: list(range(2**63 - 3, 2**63 - 1)))
show("list(range(2 ** 62, -2 ** 62, -3 * 2 ** 61))", lambda: list(range(2**62, -(2**62), -3 * 2**61)))

show("'a,b,c'.rsplit(',', 5_000_000_000)", lambda: "a,b,c".rsplit(",", 5_000_000_000))
show("b'a b c'.rsplit(None, K)", lambda: b"a b c".rsplit(None, K))
show("'aaa'.replace('a', 'b', K)", lambda: "aaa".replace("a", "b", K))

big = 2**62
show("b'ab'.center(big)", lambda: b"ab".center(big))
show("b'ab'.ljust(big)", lambda: b"ab".ljust(big))
show("b'ab'.rjust(big)", lambda: b"ab".rjust(big))
show("b'12'.zfill(big)", lambda: b"12".zfill(big))
show("bytearray(b'ab').center(big)", lambda: bytearray(b"ab").center(big))
show("b'ab'.ljust(-big)", lambda: b"ab".ljust(-big))
show("'ab'.center(big)", lambda: "ab".center(big))

show("format('abc', '.4294967296')", lambda: format("abc", ".4294967296"))
show("f'{\"abcdef\":.{K}}'", lambda: f"{'abcdef':.{K}}")
show("format('ab', '%d' % big)", lambda: format("ab", "%d" % big))
show("format(5, '0%dd' % big)", lambda: format(5, "0%dd" % big))
show("'%*s' % (big, 'a')", lambda: "%*s" % (big, "a"))
show("('%' + str(big) + 's') % 'a'", lambda: ("%" + str(big) + "s") % "a")
show("('%.' + str(K) + 's') % 'a'", lambda: ("%." + str(K) + "s") % "a")
show("'%.*s' % (K, 'abc')", lambda: "%.*s" % (K, "abc"))

show("struct.calcsize('%dx' % 2 ** 33)", lambda: struct.calcsize("%dx" % 2**33))
show("struct.calcsize('%dq' % 2 ** 33)", lambda: struct.calcsize("%dq" % 2**33))
show("struct.calcsize('%dx' % 2 ** 63)", lambda: struct.calcsize("%dx" % 2**63))
show("struct.pack('%ds' % big, b'ab')", lambda: struct.pack("%ds" % big, b"ab"))
show("struct.unpack_from('<B', b'abc', -K)", lambda: struct.unpack_from("<B", b"abc", -K))
