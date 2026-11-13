# The correctness review's low lists (A4, A5, X2): a match's repr cuts its
# group at 50 characters; a huge index of a range is only out of range;
# copy and deepcopy of a slice, of a deque subclass with attributes, and
# the refusal of builtin types that have no reduction; a subclass of a
# class standing in for a C type names itself; `__length_hint__`'s type
# message; a builtin type's `__init__` names the type in its argument
# errors.
import copy, operator, re
from collections import deque


def t(name, f):
    try:
        r = f()
    except Exception as e:
        r = f'{type(e).__name__}: {e}'
    print(name, repr(r))


for s in ['x' * 100, "it's" * 20, 'a\\b' * 30, 'é' * 60, '\n' * 30, 'short', '']:
    t('match ' + s[:3], lambda s=s: repr(re.match('.*', s, re.S)))
t('match bytes', lambda: repr(re.match(b'.*', b'\xff' * 40, re.S)))
t('match bytes q', lambda: repr(re.match(b'.*', b'q' * 70, re.S)))

t('range[1<<70]', lambda: range(10)[1 << 70])
t('range[-(1<<70)]', lambda: range(10)[-(1 << 70)])
t('list[1<<70]', lambda: [1][1 << 70])
t('str[1<<70]', lambda: 'a'[1 << 70])

t('copy slice', lambda: copy.copy(slice(1, [2])))
t('deepcopy slice', lambda: copy.deepcopy(slice(1, [2], None)))
sl = slice([1], [1])
t('deepcopy slice shares', lambda: (lambda c: c.start is c.stop)(copy.deepcopy([sl.start, sl])[1]))


def g():
    yield 1


t('deepcopy gen', lambda: copy.deepcopy(g()))
t('deepcopy keys', lambda: copy.deepcopy({1: 2}.keys()))
t('deepcopy classmethod', lambda: copy.deepcopy(classmethod(len)))
t('copy gen', lambda: copy.copy(g()))


class DQ(deque):
    pass


d = DQ([1, [2]], 5)
d.x = 7
for f in (copy.copy, copy.deepcopy):
    c = f(d)
    t(f.__name__ + ' DQ', lambda c=c: (type(c).__name__, list(c), c.maxlen, c.x, c[1] is d[1]))
t('deepcopy deque', lambda: (lambda c: (type(c).__name__, list(c), c.maxlen))(copy.deepcopy(deque([1], 2))))
t('DQ reduce', lambda: d.__reduce__()[:3])
t('deque reduce', lambda: deque([1]).__reduce__()[:3])
t('DQ name', lambda: type(d).__name__)
t('DQ attr error', lambda: d.nope)
t('deque attr error', lambda: deque().nope)
t('DQ type attr error', lambda: DQ.nope)
t('DQ concat', lambda: [] + d)


class LH:
    def __init__(self, n):
        self.n = n

    def __iter__(self):
        return iter([1, 2])

    def __length_hint__(self):
        return self.n


t('length_hint str', lambda: operator.length_hint(LH('x')))
t('length_hint -1', lambda: operator.length_hint(LH(-1)))
t('length_hint 3', lambda: operator.length_hint(LH(3)))

t('list.__init__ 2', lambda: list.__init__([], 1, 2))
t('list.__init__ kw', lambda: list.__init__([], x=1))
t('set.__init__ 2', lambda: set.__init__(set(), 1, 2))
t('set.__init__ kw', lambda: set.__init__(set(), x=1))
t('dict.__init__ 2', lambda: dict.__init__({}, 1, 2))
t('list(1, 2)', lambda: list(1, 2))
t('set(x=1)', lambda: set(x=1))
