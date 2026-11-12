# A needle of 512 KiB or more is searched a step at a time with its
# factorization kept between steps (the bytes search), and split,
# rsplit, replace, partition and rpartition by one go through `find`
# and `rfind` (docs/spec.md §5); a needle longer than the text is not
# searched at all. Results must match CPython.


def sig(x):
    if isinstance(x, str):
        return (len(x), x[:3], x[-3:]) if len(x) > 8 else x
    if isinstance(x, (list, tuple)):
        return type(x).__name__, [sig(y) for y in x]
    return x


K = 530_000
N = 'a' * K + 'b'
R = 'b' + 'a' * K
E = '\xe9' * (K // 2) + '\u20ac'
texts = [
    'a' * (2 * K),
    'c' + N + 'a' * 9 + N + 'd',
    '\xfc' * 5 + E + 'z' + E + '\xe9' * 3,
    N * 2 + 'a' * 7,
    'abc',
]
for t in texts:
    for n in (N, R, E):
        print(t.find(n), t.rfind(n), t.find(n, 3, len(t) - 1), t.rfind(n, 1, -1), n in t, n not in t)
        print(t.count(n), t.count(n, 2), t.find(n, -len(n) - 2))
        print(sig(t.split(n)), sig(t.split(n, 1)), sig(t.rsplit(n)), sig(t.rsplit(n, 1)))
        print(sig(t.replace(n, '<>')), sig(t.replace(n, '\u03a9', 1)), sig(t.replace(n, 'x', 0)))
        print(sig(t.partition(n)), sig(t.rpartition(n)))
        try:
            print(t.index(n), t.rindex(n))
        except ValueError as e:
            print('ValueError', e)
big = 'a' * 1_100_000
print(sig(big.partition('a' * 100 + 'b')), sig((big + 'xy' + big).rpartition('xy')))
print(sig(('ab' * 600_000).rsplit('b' * 1_000 + 'a')), len(('xab' * 300_000).rsplit('ab', 5)))
