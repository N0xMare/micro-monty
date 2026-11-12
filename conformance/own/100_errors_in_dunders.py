# Exceptions raised inside dunders called by builtins and operators.
class BadHash:
    def __init__(self, fail):
        self.fail = fail

    def __hash__(self):
        if self.fail:
            raise RuntimeError('hash failed')
        return 1

    def __eq__(self, o):
        raise LookupError('eq failed')


d = {BadHash(False): 'first'}
for action in (lambda: d[BadHash(True)], lambda: d[BadHash(False)], lambda: BadHash(True) in {1},
               lambda: {BadHash(False), BadHash(False)}, lambda: d.get(BadHash(False))):
    try:
        action()
    except (RuntimeError, LookupError) as e:
        print(type(e).__name__, e)
same = list(d)[0]
print(d[same], same in d)


class BadRepr:
    def __repr__(self):
        raise ValueError('repr failed')


for action in (lambda: print([1, BadRepr()]), lambda: str({'k': BadRepr()}), lambda: f'{(BadRepr(),)!r}',
               lambda: repr([[BadRepr()]])):
    try:
        action()
    except ValueError as e:
        print('ValueError', e)


class KeyFail:
    def __init__(self, v):
        self.v = v


def key(o):
    if o.v == 3:
        raise ArithmeticError(f'key failed on {o.v}')
    return o.v


for fn in (sorted, min, max):
    try:
        fn([KeyFail(1), KeyFail(3), KeyFail(2)], key=key)
    except ArithmeticError as e:
        print(fn.__name__, e)


class LtFail:
    def __lt__(self, o):
        raise TypeError('lt refused')


try:
    sorted([LtFail(), LtFail()])
except TypeError as e:
    print(e)
lst = [3, 1, 2]
try:
    lst.sort(key=lambda v: 1 / (v - 1))
except ZeroDivisionError as e:
    print('sort key error', e, sorted(lst))


class AddFail:
    def __add__(self, o):
        raise OverflowError('add failed')

    __radd__ = __add__


try:
    sum([1, AddFail()])
except OverflowError as e:
    print(e)


class InitFail:
    def __init__(self):
        raise AssertionError('init failed')


try:
    [InitFail() for _ in range(2)]
except AssertionError as e:
    print(e)


class IterFail:
    def __iter__(self):
        raise NotImplementedError('iter failed')


for action in (lambda: list(IterFail()), lambda: set(IterFail()), lambda: [*IterFail()], lambda: dict(IterFail())):
    try:
        action()
    except NotImplementedError as e:
        print(e)


class ContainsFail:
    def __contains__(self, x):
        raise IndexError('contains failed')


try:
    1 in ContainsFail()
except IndexError as e:
    print(e)
print('end')
BadRepr()
