# Number formats whose text can be long, a step at a time (docs/spec.md §5):
# a big int's power-of-two digits written into their str's block
# (`format`, f-strings, `bin`/`oct`/`hex`, and `%x %X %o` past a million
# digits through `_percent`), numbers padded to a width of millions,
# CPython's grouped zero padding, a lone f-string field without a copy, and
# true division of big ints. Results must equal CPython's.
import hashlib


def fp(s):
    if isinstance(s, bytes):
        s = s.decode()
    return len(s), s[:16], s[-16:], hashlib.sha256(s.encode()).hexdigest()[:12]


def show(label, f):
    try:
        r = f()
    except Exception as e:
        r = (type(e).__name__, str(e)[:70])
    print(label, r)


for v in [0, 1, -1, 7, 255, -256, 2**64, -(2**100) + 3, 3**700, -(5**900)]:
    print(bin(v), oct(v), hex(v), format(v, '#_x'), f'{v:+_o}')

# Power-of-two digits past a million, then past a step (2^21 digits).
big = (1 << 2_200_000) // 7 + 12345
for v in (big, -big):
    print(fp(bin(v)), fp(oct(v)), fp(hex(v)))
    for spec in ['b', 'o', 'X', '#b', '#x', '+x', ' o', '_b', '#_X', '+#_o',
                 '>3000000b', '^3000000o', '=+3000000x', '*^4000000_x', '03000000_x',
                 '#03000000_b', '0=4000000o', 'é>2500000X', '0900000x']:
        show(spec, lambda: (fp(format(v, spec)), format(v, spec) == f'{v:{spec}}'))

n = (1 << 4_200_000) // 7


class I(int):
    pass


class Ix:
    def __index__(self):
        return (1 << 4_200_001) + 99


for v in (n, -n, I(n), Ix()):
    for p in ['%x', '%X', '%o', '%#x', '%#o', '%+x', '% o', '%+#X', '%4300000x',
              '%-4300000o', '%04300000x', '%#04300000X', '%.5x', '%.1100000x', '%.1500000o']:
        show(p, lambda: (fp(p % v), fp(p.encode() % v)))
print(fp('%x-%o|%s|%X' % (n, n, 'mid', -n)))
print(fp('%(a)x %(b)#o' % {'a': n, 'b': -n}))

# Numbers padded to a width of millions, and CPython's grouped zeros.
W = '8500000'
for v in [-5, True, 10**3000 + 7, -0.0, float('inf'), 12345.678, 1 + 2j]:
    for spec in ['>' + W, '^' + W, '+=' + W, '0' + W + ',', '#0' + W + '_x', 'é=' + W, '0' + W + 'n', '0' + W + ',.2f']:
        show(spec, lambda: (fp(format(v, spec)), format(v, spec) == f'{v:{spec}}'))
for v, spec in [(65, '>' + W + 'c'), (43, '0' + W + 'c'), (-5, 'z' + W), (5, '0' + W + '.2d'),
                (5, W + ',x'), (-1.5, '0' + W + ',.3_f'), (2.5e-10, '0' + W + '_g')]:
    show(spec, lambda: fp(format(v, spec)))

# A lone f-string field is the formatted str itself.


class SF(str):
    def __format__(self, spec):
        return 'F' + spec


s = 'abc'
print(f'{s}' is s, f'{s!s}' is s, f'{s:}' is s, f'{SF("q")}', f'{SF("q"):>3}', f'{5}', f'{s!r}')
L = ['x' * 3_000_000, 'é']
print(fp(f'{L}'), fp(f'{L[0]:>9000000}'))

# True division of big ints rounds once, as CPython's.
a = (1 << 3_000_000) - 1
b = (1 << 2_999_000) + 12345
print(a / b, -a / b, b / a, (a >> 2_999_990) / 3, a // b, a % b == a - (a // b) * b)
for x, y in [(1, 3 << 1100), (-(1 << 1100), 3), ((1 << 1077) + 1, 1 << 2150), (3, 1 << 1076),
             (5 << 1000, (1 << 1000) + 1), ((1 << 1100) + (1 << 1046), 1 << 1046)]:
    show('div', lambda: x / y)
show('div big', lambda: (1 << 2000) / 7)
