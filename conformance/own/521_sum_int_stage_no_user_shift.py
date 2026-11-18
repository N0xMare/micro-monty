# `sum` over a generator: the int stage tests each sum natively, so a
# `__radd__` result's `__rshift__` never runs, and the stage ends where
# CPython's does: at an item that is not an int or does not fit i64 (also
# when the sum fits again), or a sum that does not. Floats after the stage
# show where it ended (Neumaier compensation only in the float stage).
# Totals past 2**47 (heap ints, not small ints) and int subclasses
# with their own dunders take the native test's other path.
log = []


class R:
    def __init__(self, v): self.v = v
    def __radd__(self, o): log.append('radd'); return Y(o + self.v)
    def __add__(self, o): log.append('add'); return Y(self.v + o)


class Y:
    def __init__(self, v): self.v = v
    def __add__(self, o): log.append('yadd'); return Y(self.v + (o.v if isinstance(o, R) else o))
    def __rshift__(self, n): log.append('rshift'); return 0
    def __repr__(self): return f'Y({self.v})'


class B:
    def __radd__(self, o): log.append('Bradd'); return True


class I(int):
    def __radd__(self, o): log.append('Iradd'); return I(int(o) + int(self))
    def __rshift__(self, n): log.append('Irshift'); raise ValueError('no')


def g(xs):
    yield from xs


def t(xs, start=0):
    log.clear()
    try:
        r = sum(g(xs), start)
    except Exception as e:
        r = f'{type(e).__name__}: {e}'
    print(repr(r), type(r).__name__, log)


F = [1e100, 1.0, -1e100]
t([1, 2, R(3), 4])
t([1, I(2), 3])
t([1, I(2)] + F)
t([1, 2] + F)
t([2**62, 2**62] + F)
t([-10, 2**63] + F)
t([10, -2**63 - 5] + F)
t([True, 2, 3.5] + F)
t([1, 'x'])
t([1.5, 2])
t([1, 2j, 3])
t([], 5)
t([2**63, 1])
t([-2**63, -1, 0.5])
t([1, 2, 3], -2**63)
t(list(range(1000)) + F)
t([-2**62, 2**63] + F)
t([2**62 - 1, -2**63 - 1] + F)
t([2**62, 2**62 - 1, -2**63] + F)
t([2**63 - 1, -2**63 - 1, -2**63] + F)
t([2**63 - 1, -2**63 - 2] + F)
t([2**62, -1, -2**62, 5] + F)
t([1, R(3), 4], 2**62)
t([True, False, True], -2**62)
t([2**62 - 1, 1, -1, 2**62, -2**62] + F)
t([2**47 - 1, 1, 2] + F)
t([2**47, 1, 2] + F)
t([-2**47 - 1, -1, 3] + F)
t([2**50, 2**50, -2**51, 7] + F)
t([2**40] * 300 + F)
t([2**48, R(3), 4])
t([2**48, I(2), 3])
t([1, R(3), 4], 2**50)
t([1, I(2), 3] + F, -2**50)
t([2**47, True, 2**63] + F)
t([1, B()] + F)
t([2**48, B()] + F)
t([1, B(), 2, 3])
