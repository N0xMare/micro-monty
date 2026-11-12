# `copy.copy` and `copy.deepcopy` of a bytearray give an equal new one
# (a long one copied a step at a time, past 8 MiB), aliasing kept inside
# containers. `format`, a lone f-string field and a one-field `str.format`
# give what `__format__` returned, a str subclass included; a joined field
# is its exact str, as CPython's.
import copy

small = bytearray(b'abc')
big = bytearray(b'xyz') * 2_900_000
for f in (copy.copy, copy.deepcopy):
    for b in (small, big):
        c = f(b)
        print(f.__name__, type(c).__name__, len(c), c == b, c is b, c[:3], c[-2:])
        c[0] = 0
        print(b[0], c[0])
items = [small, 1, small, {'k': small}]
d = copy.deepcopy(items)
print(d[0] is d[2] is d[3]['k'], d[0] is small, d[0] == small)
s = copy.copy(items)
print(s[0] is small, s is items)
print(copy.deepcopy(bytearray()), copy.copy(bytearray(b'\x00\xff')))


class S(str):
    def __str__(self):
        return 'STR'


class X:
    def __init__(self, r):
        self.r = r

    def __format__(self, spec):
        return self.r


s = S('hi')
e = S('')
x, y = X(s), X(e)
cases = (
    ("f'{x}'", f'{x}'), ("f'{x:>3}'", f'{x:>3}'),
    ("f'<{x}>'", f'<{x}>'), ("f'{x}{x}'", f'{x}{x}'), ("f'{y}'", f'{y}'),
    ("format(x)", format(x)), ("format(x, 'q')", format(x, 'q')),
    ("format(y)", format(y)), ("format(s)", format(s)), ("f'{s}'", f'{s}'),
    ("'{}'.format(x)", '{}'.format(x)), ("'{0}'.format(x)", '{0}'.format(x)),
    ("'{:z}'.format(x)", '{:z}'.format(x)), ("'{}{}'.format('', x)", '{}{}'.format('', x)),
    ("'{}{}'.format(x, '')", '{}{}'.format(x, '')), ("'a{}'.format(x)", 'a{}'.format(x)),
    ("'{}'.format(y)", '{}'.format(y)), ("'{}'.format(s)", '{}'.format(s)),
    ("'%s' % (s,)", '%s' % (s,)),
)
for name, r in cases:
    print(name, type(r).__name__, r is s, repr(r))
n = 0
for k in range(3):
    n += len(f'{x}') + len(f'[{x}]')
print(n)
