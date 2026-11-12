# Long reprs and copies go a step at a time (docs/spec.md §5): a long
# str, bytes or bytearray repr is built in steps (a bytearray's escapes
# every `'`, as CPython's), `format(s)` of a str is the str, and a long
# copy made by `bytes()`, `bytearray()` or `bytearray.copy()` steps too.
# Sized just over the step thresholds so a park lands inside each; results
# must equal one native run's.


def h(t):
    m = len(t) // 2
    return len(t), t[:12], t[m:m + 12], t[-12:]


s = 'ab' * 1_050_000
u = 'xé' * 700_000 + "'"
print(format(s) is s, format(s, '') is s)
print(h(f'<{s}|{s}>'), h(f'{u!r}'), h('%r' % (u,)))

for B, d in (
    (bytes, b"a'b"),
    (bytearray, b"a'b"),
    (bytearray, b'q\'x"z'),
    (bytearray, b'\x00\x7f\x80\xff\\\n'),
):
    v = B(d * (2_100_000 // len(d)))
    r = repr(v)
    print(B.__name__, h(r), '%r' % (v,) == r, h(f'[{v}|{v}]'))
v = bytearray(b"a'" * 1_050_000)
print(str(v) == ascii(v) == f'{v!r}' == f'{v!a}' == repr(v))
print(repr(bytearray()), f'{bytearray(b"x")}', f'{b""!r}')

b = bytes(range(256)) * 33_000
for c in (bytearray(b), bytes(bytearray(b)), bytearray(b).copy(), bytearray(b[1:])):
    print(type(c).__name__, len(c), c[:3], c[-3:], c == b or c == b[1:], c is b)
c = bytearray(b)
c[0] = 9
print(b[0], c[0], bytes(b) is b)
