# NFKC-normalized identifiers (ligatures, math letters, fullwidth, long s,
# ordinal indicators), starred class bases, del of tuple and list targets
# and parenthesized with-items.
ﬁle = 1
print(file)
𝐀 = 2
print(A)
ſ = 3
print(s)
ª = 4
print(a)
µ = 5
print(μ)
ｘ = 6
print(x)

bases = (dict,)
class C(*bases, *()):
    pass
print(C.__mro__[1].__name__, C(a=1))
a, b, c = 1, 2, 3
del (a, b)
print('a' in globals(), 'b' in globals(), c)
a, b = 1, 2
del [a, (b, c)]
print([n in globals() for n in 'abc'])


class CM:
    def __init__(self, n):
        self.n = n
    def __enter__(self):
        print('enter', self.n)
        return self.n
    def __exit__(self, *e):
        print('exit', self.n)


with (CM(1) as x, CM(2) as y):
    print(x, y)
with (CM(3), CM(4)):
    pass
with (CM(5) as z,):
    print(z)
