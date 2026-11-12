# Bounded native steps for str (docs/spec.md §5): long `*`, padding,
# formatting, `join`, `replace`, splits, case maps, `translate`,
# `expandtabs`, `count`, predicates, extended slices, `repr`, long
# `strip` runs, `str.format` of a long template, the repr of a long
# `bytes`, and long copies (`s + t`, `s[a:b]`, `removeprefix`,
# `removesuffix`, a `strip` result, a long split or partition piece)
# go a step at a time,
# writing their result into its arena block a slice per step.
# Each input here is past the size where that starts, so a park lands
# inside the operation; results must equal CPython's.
def h(x):
    # A digest of a long result: its length and slices across it (no
    # hashing: this case also runs in debug builds).
    if isinstance(x, (list, tuple)):
        x = '\x1f'.join(x) + '#' + str(len(x))
    if isinstance(x, bytes):
        x = x.decode('latin-1')
    if isinstance(x, str):
        n = len(x)
        return (n, x[:12], x[-12:]) + tuple(x[n * k // 7 + k:n * k // 7 + k + 9] for k in range(1, 7))
    return x


def show(label, f):
    try:
        print(label, h(f()))
    except Exception as e:
        print(label, type(e).__name__, e)


A = 'ab' * 150_000
U = 'aé\u2028Σx\tΣ. ' * 60_000
show('replace', lambda: A.replace('a', 'xyz'))
show('replace count', lambda: A.replace('b', 'é', 140_001))
show('replace sigma', lambda: U.replace('Σ', 'σσ'))
show('replace empty', lambda: ('é' * 150_000).replace('', '-', 140_000))
show('split', lambda: A.split('b'))
show('split max', lambda: A.split('b', 140_000))
show('split ws', lambda: U.split())
show('split ws max', lambda: U.split(None, 140_000))
show('rsplit', lambda: A.rsplit('a'))
show('rsplit max', lambda: A.rsplit('a', 140_000))
show('rsplit ws', lambda: (' x  y\t' * 80_000).rsplit())
show('splitlines', lambda: ('a\r\nb\rc\n\x0bd\x85' * 30_000).splitlines(True))
show('splitlines tail', lambda: ('ab\n' * 140_000 + 'tail').splitlines())
show('join list', lambda: ','.join([str(i) for i in range(150_000)]))
show('join tuple', lambda: 'é|'.join(('a', 'bé') * 75_000))
show('join iter', lambda: ', '.join(iter(['p', 'q'] * 75_000)))
show('str of lists', lambda: str([[i] for i in range(150_000)]))
for m in ('upper', 'lower', 'swapcase', 'title', 'capitalize', 'casefold'):
    show(m, lambda: getattr(U, m)())
show('lower sigma', lambda: ('ΑΣ\u0301 Σ ΣΣ aΣ.b' * 50_000).lower())
show('lower sigma far', lambda: ('A' + 'Σ' + "'" * 600_000 + 'b').lower())
show('title words', lambda: ("they're bill's 3rd ǆ x é" * 25_000).title())
show('translate', lambda: U.translate({97: 'xy', ord('é'): None, ord('Σ'): 0x3c3}))
show('translate twin', lambda: U.translate({97: [1]}))
show('expandtabs', lambda: ('é\tbc\t\tx\n\tx\r\t' * 50_000).expandtabs())
show('count', lambda: A.count('a'))
show('count window', lambda: A.count('ab', 5, -7))
D = '0123456789' * 450_000
for p in ('isdigit', 'isprintable', 'isidentifier', 'isupper', 'istitle'):
    show(p, lambda: (getattr(D, p)(), getattr('é' * 2_300_000, p)()))
show('slice rev', lambda: D[::-1])
show('slice step', lambda: ('é' * 2_300_000 + 'aé' * 1_000_000)[-2::-3])
show('slice far', lambda: [('aé' * 3_000_000)[i::k] for k in (2 ** 62, -2 ** 63, 999_999) for i in (None, 1, -1)])
show('repr', lambda: repr("it's \"q\"\\\x00\u2028" * 200_000))
show('ascii', lambda: ascii('aé\U0001f600' * 300_000))
show('repeat', lambda: 'xé' * 4_000_000)
show('ljust', lambda: 'abc'.ljust(10_000_000, 'é'))
show('zfill', lambda: '-42'.zfill(12_000_000))
show('format', lambda: f"{'é':^12000001}")
show('percent', lambda: '<%12000000s|%-3s>' % ('a', 'b'))
show('concat', lambda: 'x' * 9_000_000 + 'é')
W = ' \t\u3000\x85' * 1_200_000
show('strip', lambda: (W + 'aé b' + W).strip())
show('lstrip', lambda: (W + 'x' + W).lstrip())
show('rstrip', lambda: (W + 'x' + W).rstrip())
show('strip all', lambda: W.strip())
show('strip chars', lambda: ('ab' * 2_000_000 + 'é' + 'ba' * 2_000_000).strip('ab'))
show('strip none', lambda: ('é' * 3_000_000).strip('xyz'))
show('format plain', lambda: ('é' * 3_000_000).format())
show('format fields', lambda: ('{}' + 'é' * 2_000_000 + '{!r:>9}{{x}}' + 'a' * 2_000_000 + '{k}}}').format('p', 'q', k='é'))
show('format numbering', lambda: ('{}' + 'é' * 2_000_000 + '{0}').format('p'))
show('format error', lambda: ('é' * 3_000_000 + '{').format())
R = {i: [i, 'é', (i,), {i % 3}] for i in range(3_000)}
R['me'] = R
show('repr twin', lambda: repr([R, R.keys(), frozenset({1, 2}), R]))
show('ascii twin', lambda: ascii([('é', i) for i in range(5_000)] + [{1, 2}]))
Bb = bytes(range(256)) * 9_000
show('bytes repr', lambda: repr(Bb))
show('bytes fstr', lambda: (f'{Bb}', f'{Bb!a}', str([Bb])[:30]))
show('bytes quotes', lambda: (repr(b"it's" * 600_000), repr(b"'\"" * 600_000)))
show('bytes pct plain', lambda: (b'ab' * 600_000) % ())
show('bytes pct error', lambda: (b'ab' * 600_000) % (1,))
show('bytes pct spec', lambda: (b'ab' * 600_000 + b'%s%%') % (b'q',))
show('pct long', lambda: ('%%' * 600_000 + '%s' + 'é' * 600_000) % ('q',))
show('join one', lambda: ''.join(['é' * 2_000_000]))
J = [('%06d' % i).encode() for i in range(140_000)]
show('bytes join', lambda: b'-'.join(J))
show('bytes join tuple', lambda: b''.join(tuple(J)))
show('bytes join gen', lambda: b', '.join(x for x in J))
show('bytes join mixed', lambda: b'-'.join(J + [bytearray(b'q')]))
show('bytes join error', lambda: b'-'.join(J + ['s']))
show('bytes join one', lambda: b'-'.join([b'z' * 3_000_000]))
show('concat wide', lambda: 'é' * 3_000_000 + 'a' * 3_000_000)
show('slice copy', lambda: ('é' + 'a' * 9_000_000 + 'é')[1:-1])
show('removeprefix', lambda: ('é' + 'a' * 9_000_000).removeprefix('é'))
show('removesuffix', lambda: ('a' * 9_000_000 + 'é').removesuffix('é'))
show('strip copy', lambda: (' ' + 'é' * 4_600_000 + ' ').strip())


class Left:
    def __add__(self, other):
        return other


# Callers that take a value at once (no steps) get the whole result.
show('concat at once', lambda: sum(['é' * 4_500_000, 'x' * 100], Left()))
import math
show('repeat at once', lambda: math.prod([3_000_000, 3], start='xé'))


class PU:
    def __str__(self):
        return 'Ué'

    def __index__(self):
        return 7


L = 'é' * 70_000
show('pct steps', lambda: (L + '%s|%5d|%%%c' * 20_000 + L) % (('w', 42, 'ü') * 20_000))
show('pct steps py', lambda: ('%s,' * 40_000) % tuple(PU() if i % 9_999 == 5 else i for i in range(40_000)))
show('pct steps x', lambda: ('%x;' * 30_000) % tuple(PU() if i == 29_000 else i for i in range(30_000)))
show('pct steps map', lambda: (L + '%(a)s' + L + '%(b)r') % {'a': 'A', 'b': 'é'})
show('pct steps pct', lambda: ('%%' * 200_000 + '%s') % ('q',))
show('pct steps bytes', lambda: (b'a' * 200_000 + b'%s%%%b' * 1_000) % ((b'x', b'y') * 1_000))
show('pct steps error', lambda: (L + '%s' * 5 + L + '%y') % (1, 2, 3, 4, 5, 6))
show('pct steps short', lambda: (L + '%s%s') % ('x',))
show('pct steps big spec', lambda: ('%' + '0' * 150_000 + '5d' + L) % 42)
show('pct steps big key', lambda: ('%(' + 'k' * 150_000 + ')s' + L) % {'k' * 150_000: 'V'})


class Sep(str):
    pass


P = 'a' * 9_000_000
show('split piece', lambda: (P + ',b,' + 'é' * 4_600_000).split(','))
show('rsplit ws piece', lambda: ('  ' + 'é' * 4_600_000 + ' x \t' + P + ' ').rsplit(None, 1))
show('partition piece', lambda: (P + '-é-' + 'é' * 4_600_000).partition('-é-'))
show('rpartition piece', lambda: [type(x).__name__ for x in (P + 'xy' + P + 'xy').rpartition(Sep('xy'))])
