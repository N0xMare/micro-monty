# A str or bytes repeat whose length would pass sys.maxsize is an
# OverflowError (before any allocation), as in CPython; below it, or for
# list, tuple and bytearray, a result too big for memory is a MemoryError.
# The str length counts code points, not bytes. Found by difffuzz (seed
# 30837161: '  0.00' * pow(1000, 21, 2 ** 61 - 1)).
import sys


def show(label, f):
    try:
        r = f()
        print(label, type(r).__name__, len(r))
    except (OverflowError, MemoryError) as e:
        print(label, type(e).__name__, e)


big = sys.maxsize // 2 + 1  # 2 ** 62 on 64-bit builds
show("'ab' * big", lambda: "ab" * big)
show("big * 'ab'", lambda: big * "ab")
show("'a' * big", lambda: "a" * big)
show("'\\xe9' * big", lambda: "\xe9" * big)
show("'\\xe9\\xe9' * big", lambda: "\xe9\xe9" * big)
show("'\\U0001f600' * big", lambda: "\U0001f600" * big)
show("'' * sys.maxsize", lambda: "" * sys.maxsize)
show("'ab' * sys.maxsize", lambda: "ab" * sys.maxsize)
show("b'ab' * big", lambda: b"ab" * big)
show("big * b'ab'", lambda: big * b"ab")
show("bytearray(b'ab') * big", lambda: bytearray(b"ab") * big)
show("[1, 2] * big", lambda: [1, 2] * big)
show("(1, 2) * big", lambda: (1, 2) * big)
show("'  0.00' * pow(1000, 21, 2 ** 61 - 1)", lambda: "  0.00" * pow(1000, 21, 2**61 - 1))
s = "ab"
try:
    s *= big
except OverflowError as e:
    print("s *= big", type(e).__name__, e)
b = bytearray(b"ab")
try:
    b *= big
except MemoryError as e:
    print("b *= big", type(e).__name__, e)
print(s, bytes(b))
