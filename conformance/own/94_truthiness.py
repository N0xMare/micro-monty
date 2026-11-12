# __bool__ / __len__ in conditions, boolean operators and builtins.
log = []


class B:
    def __init__(self, v, name):
        self.v, self.name = v, name

    def __bool__(self):
        log.append(self.name)
        return self.v

    def __repr__(self):
        return self.name


class L:
    def __init__(self, n):
        self.n = n

    def __len__(self):
        log.append(f'len{self.n}')
        return self.n


t, f = B(True, 't'), B(False, 'f')
print(t and f, f and t, t or f, f or t, not t, not f, log)
log.clear()
r = [x for x in [t, f, L(0), L(2)] if x]
print(r == [t, r[1]], log)
log.clear()
n = 0
w = L(3)
while w:
    w.n -= 1
    n += 1
print(n, log)
log.clear()
print(any([f, L(0), t]), all([t, L(1), f]), log)
log.clear()
print(list(filter(None, [f, t, L(0), L(4)]))[0], log)
log.clear()
print(t if f else 'else-branch', bool(L(0)), bool(L(7)), log)
log.clear()
x = f or L(0) or 'last'
print(x, log)
log.clear()
print(1 < 2 and t, (f or 0), log)


class Both:
    def __bool__(self):
        return False

    def __len__(self):
        return 10


print(bool(Both()), len(Both()), 'yes' if Both() else 'no')


class BadBool:
    def __bool__(self):
        return 'yes'


for expr in (lambda: bool(BadBool()), lambda: not BadBool(), lambda: BadBool() and 1, lambda: [1 for _ in [0] if BadBool()]):
    try:
        expr()
    except TypeError as e:
        print(e)


class RaisingBool:
    def __bool__(self):
        raise KeyError('bool failed')


try:
    if RaisingBool():
        pass
except KeyError as e:
    print('KeyError', e)


class NegLen:
    def __len__(self):
        return -5


try:
    if NegLen():
        pass
except ValueError as e:
    print(e)
assert t, 'never'
try:
    assert f, 'assert message'
except AssertionError as e:
    print('AssertionError', e)
print(log[-2:])
