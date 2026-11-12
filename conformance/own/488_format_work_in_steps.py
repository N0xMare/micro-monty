# Formatting is budgeted by work, not by length or visits: a short `%`
# format, `str.format` or container `repr` over many big ints ends its
# steps by the conversions' work (each charged by its size). `%r`, `%a`
# and `%s` of long texts take the stepped `repr`, a bytearray is a piece,
# and a huge precision's zeros past a double's exact digits (or an int's
# zero fill) are a piece, unwritten. Results must equal CPython's.

L = [10**4000] * 16 + [-(10**4200), 7]
t = ('%d,' * len(L)) % tuple(L)
print(len(t), t == ','.join(map(str, L)) + ',')
print(('%s|' * 3) % (L[0], L[-2], [L[0], 1]) == f'{L[0]}|{L[-2]}|{[L[0], 1]}|')
r = repr(L)
print(len(r), r == '[' + ', '.join(map(str, L)) + ']')
d = {i: L[i] for i in range(18)}
print(len(f'{L}'), str(d) == '{' + ', '.join(f'{i}: {L[i]}' for i in range(18)) + '}')
print(('{}' * len(L)).format(*L) == ''.join(map(str, L)), ('{}-' * 3).format(*L[:3])[:5])

S = 'a\n' * 1_050_000
E = 'é\x01' * 700_000
for t in ('%r' % S, '%a' % E, '%s' % [S], '%5r|' % 'x', '%-.3a' % E, '%10.4r|' % E):
    print(len(t), ascii(t[:8]), ascii(t[-8:]))
B = bytearray(b'xy') * 1_100_000
print(len(b'%s%s' % (B, B)), (b'%s%s' % (B, b'!'))[-4:], (b'%r' % B)[:20])
print(len(b'%a' % bytes(B)), (b'%4s|' % bytearray(b'q')))

for f in ('%.1100000f', '%#.1100000g', '%+20.1100000E', '%-1100100.1100000F'):
    for x in (-2.5e-300, 1e300, float('inf')):
        t = f % x
        print(f, len(t), t[:12], t[-12:], t.count('0'))
for f in ('%+.1100000x', '%#.1100000o', '%-1100100.1100000d', '%01100000.1d'):
    for x in (-12345, 10**100):
        t = f % x
        print(f, len(t), t[:8], t[-8:])
print(len(b'%.1100000f' % 1.5), (b'%.1100000d' % -7)[:3], (b'%.1100000e' % 2.0)[-6:])

# A key call after one that did long work, and `print` past a step's work.
import sys
a = [10**4298 + (i * 7919) % 101 for i in range(60)]
b = sorted(a, key=str)
print(b == sorted(a), b[0] % 1000, b[-1] % 1000)
print(*a[:30], sep='|', end='$\n')
sys.set_int_max_str_digits(0)
c = [10**9000 * (i % 3) + i for i in range(8)]
print(len(('%d,' * 8) % tuple(c)), len(repr(c)), len(('{}' * 8).format(*c)))
print(sorted(c, key=str)[:3] == sorted(c, key=lambda x: str(x))[:3])
sys.set_int_max_str_digits(4300)
