# Recursion through dunders and prelude code hits RecursionError, and recovers.
class Loop:
    def __repr__(self):
        return repr(self)


try:
    repr(Loop())
except RecursionError as e:
    print('repr:', e)


class EqLoop:
    def __eq__(self, o):
        return self == o

    __hash__ = object.__hash__


try:
    EqLoop() == 1
except RecursionError as e:
    print('eq:', e)


class Deep:
    def __init__(self, n):
        self.child = Deep(n - 1) if n else None


try:
    Deep(10000)
except RecursionError as e:
    print('init:', e)
print('still alive', Deep(5).child.child is not None)


class AddLoop:
    def __add__(self, o):
        return self + o


try:
    AddLoop() + 1
except RecursionError as e:
    print('add:', e)


class GetattrLoop:
    def __getattr__(self, name):
        return getattr(self, name + 'x')


try:
    GetattrLoop().a
except RecursionError as e:
    print('getattr ok')


class LenLoop:
    def __len__(self):
        return len(self)


try:
    bool(LenLoop())
except RecursionError as e:
    print('len:', e)


def gen_rec(n):
    yield n
    yield from gen_rec(n + 1)


try:
    for x in gen_rec(0):
        pass
except RecursionError as e:
    print('yield from depth reached', x > 10)


class KeyLoop:
    def __hash__(self):
        return hash(self)


try:
    {KeyLoop(): 1}
except RecursionError as e:
    print('hash:', e)


class Nested:
    def __init__(self, inner):
        self.inner = inner

    def __repr__(self):
        return f'N({self.inner!r})'


n = None
for i in range(20):
    n = Nested(n)
print(repr(n)[:30])
x = []
for i in range(5000):
    x = [x]
try:
    repr(x)
except RecursionError as e:
    print('nested list repr:', e)


class SortLoop:
    def __lt__(self, o):
        return sorted([self, o])[0] is self


try:
    sorted([SortLoop(), SortLoop()])
except RecursionError as e:
    print('sort:', e)


class Fact:
    def __call__(self, n):
        return 1 if n <= 1 else n * self(n - 1)


print(Fact()(20) > 0)


class ReprLoop2:
    def __repr__(self):
        return f'{[self]}'


ReprLoop2()
print('done')
repr(ReprLoop2())
