# `sys.stdout.writelines` and `sys.stderr.writelines` (a list or tuple
# natively, any other iterable pulled a line at a time; a non-str line
# fails after the lines before it), `sys.exit`, and a dict subclass's
# `__missing__` through `d.__getitem__(k)` and `dict.__getitem__(d, k)`.
import difflib
import sys
from collections import Counter, defaultdict

out = sys.stdout
out.writelines(['a\n', 'b'])
out.writelines(x + '\n' for x in 'cd')
out.writelines(iter(('e', 'f\n')))
out.writelines({'g\n': 1})
out.writelines(('h', '\n'))
print(out.writelines([]), out.writelines(()))
sys.stderr.writelines(['to stderr\n'])
for bad in ([1], ['i', 2], ('j', None), 5, None, (x for x in ['k', b'l'])):
    try:
        out.writelines(bad)
    except TypeError as e:
        print('|TypeError', e)
out.writelines(difflib.unified_diff(['one\n', 'two\n', 'three\n'], ['one\n', 'TWO\n', 'three\n', 'four\n'],
                                    'a.txt', 'b.txt'))
out.writelines(['x' * 3] * 5 + ['\n'])

for args in ((), (None,), (0,), (3,), ('bye',), ((1, 2),), ((),), ([1],), (1, 2)):
    try:
        sys.exit(*args)
    except SystemExit as e:
        print('SystemExit', repr(e.code), e.args)
    except TypeError as e:
        print('TypeError', e)


class D(dict):
    def __missing__(self, k):
        return ('miss', k)


class K:
    def __init__(self, v):
        self.v = v

    def __hash__(self):
        return hash(self.v)

    def __eq__(self, o):
        return isinstance(o, K) and o.v == self.v

    def __repr__(self):
        return f'K({self.v})'


d = D(a=1)
d[K(1)] = 'k1'
for k in ('zz', 'a', K(1), K(2), 5):
    print(d[k], d.__getitem__(k), dict.__getitem__(d, k), D.__getitem__(d, k))
print(list(map(d.__getitem__, ['a', 'q'])), list(map(dict.__getitem__, [d, d], ['a', K(9)])))


class E(dict):
    pass


for f in (lambda: E(a=1).__getitem__('x'), lambda: dict.__getitem__(E(), K(3)),
          lambda: dict.__getitem__({}, 'q')):
    try:
        f()
    except KeyError as e:
        print('KeyError', e)


class G(dict):
    def __getitem__(self, k):
        return 'G'

    def __missing__(self, k):
        return 'Gm'


gg = G(a=1)
print(gg['x'], gg.__getitem__('x'), dict.__getitem__(gg, 'x'), dict.__getitem__(gg, 'a'))


class H(dict):
    def __missing__(self, k):
        raise ValueError(k)


try:
    H().__getitem__('v')
except ValueError as e:
    print('ValueError', e)
dd = defaultdict(list)
print(dd.__getitem__('x'), dict.__getitem__(dd, 'y'), sorted(dd), Counter('ab').__getitem__('z'),
      dict.__getitem__(Counter(), 'q'))

import urllib.parse

print(urllib.parse.quote('a b/ü~?'), urllib.parse.quote_from_bytes(b'\x00 \xff/x', safe=''),
      urllib.parse.quote_plus('x y+z'), urllib.parse.quote('é' * 300)[-12:])
