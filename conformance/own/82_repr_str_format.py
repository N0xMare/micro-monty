# __repr__, __str__, __format__ and containers holding user objects.
class P:
    def __init__(self, x):
        self.x = x

    def __repr__(self):
        return f'P({self.x!r})'


class S(P):
    def __str__(self):
        return f'<S {self.x}>'


class F:
    def __format__(self, spec):
        return f'F[{spec}]'


p, s = P('a'), S(2)
print(p, s, str(p), repr(s), [p, s], (p,), {p: s}, {'k': [s]})
print(f'{p} {s} {s!r} {p!s} {F():>10} {F()}', format(F(), 'x'), '{} {!r} {:q}'.format(s, s, F()))
print('{0!s} and {0!r}'.format(s), '{}'.format(p))
print(str([s]), repr([s]), ascii(P('é')), f'{[s]}')


class Plain:
    pass


q = Plain()
r = repr(q)
print(r.startswith('<__main__.Plain object at 0x'), str(q) == r, int(r[r.index(' at ') + 4:-1], 16) == id(q))
try:
    format(Plain(), 'x')
except TypeError as e:
    print(e)
print(format(Plain(), '') .startswith('<__main__.Plain'))


class BadRepr:
    def __repr__(self):
        return 42


try:
    repr(BadRepr())
except TypeError as e:
    print(e)
try:
    print([BadRepr()])
except TypeError as e:
    print(e)


class BadStr:
    def __str__(self):
        raise ValueError('no str')


try:
    print(BadStr())
except ValueError as e:
    print('ValueError', e)
print(repr(BadStr()).startswith('<__main__.BadStr'))


class Rec:
    def __init__(self):
        self.children = []

    def __repr__(self):
        return f'Rec({self.children})'


a = Rec()
a.children.append(Rec())
a.children.append([1, Rec()])
print(a)
lst = [P(1)]
lst.append(lst)
print(lst)
d = {'self': None}
d['self'] = d
d['p'] = P(d)
print(d)


class Fmt:
    def __init__(self, v):
        self.v = v

    def __format__(self, spec):
        if not spec:
            return str(self)
        return format(self.v, spec)

    def __str__(self):
        return 'Fmt!'


print(f'{Fmt(3.14159):.2f}|{Fmt(5):04d}|{Fmt(1)}')


class Echo:
    def __repr__(self):
        return 'Echo()'


print(Echo())
print(*[Echo(), P(1)], sep=' | ', end='!\n')
Echo()
