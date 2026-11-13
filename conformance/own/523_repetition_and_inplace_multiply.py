# Item 13 and R1: repetition and in-place multiply over sequence kinds,
# counts and forms. `x *= seq` repeats `seq` only when `x`'s type has no
# sequence slots: an int subclass, an `__index__` class, a range, dict or
# set on the left refuses, a deque on the right included. `dq *= R()`
# runs `R.__rmul__`. A deque of two or more items times 2**62 raises
# MemoryError, bounded or not, as CPython's size check does. (Not here:
# the dunders as attributes, empty bytes' identity and float identity,
# which differ.)
import operator
from collections import deque
class IS(int): pass
class Ix:
    def __index__(self): return 2
class Bad:
    def __index__(self): raise ValueError('bad index')
class R:
    def __rmul__(self, o): return ('rmul', type(o).__name__)
    def __mul__(self, o): return ('mul', type(o).__name__)
class LS(list): pass
class SS(str): pass
class TS(tuple): pass
class BS(bytes): pass
def kinds():
    return [('list', lambda: [1, 2]), ('tuple', lambda: (1, 2)), ('str', lambda: 'ab'), ('bytes', lambda: b'ab'),
            ('bytearray', lambda: bytearray(b'ab')), ('deque', lambda: deque([1, 2])), ('dqmax', lambda: deque([1, 2], maxlen=3)),
            ('LS', lambda: LS([1, 2])), ('SS', lambda: SS('ab')), ('TS', lambda: TS((1, 2))), ('BS', lambda: BS(b'ab')),
            ('empty list', lambda: []), ('empty str', lambda: ''), ('empty tuple', lambda: ()),
            ('range', lambda: range(3)), ('dict', lambda: {1: 2}), ('set', lambda: {1}), ('int', lambda: 3)]
counts = [('0', 0), ('1', 1), ('2', 2), ('-1', -1), ('True', True), ('False', False), ('2**62', 2**62), ('2**63', 2**63),
          ('2**64', 2**64), ('IS2', IS(2)), ('Ix', Ix()), ('Bad', Bad()), ('1.5', 1.5), ('None', None), ('R', R()), ('str', 'x')]
def show(r, s):
    if isinstance(r, (list, tuple, str, bytes, bytearray, deque, range, dict, set)) and len(r) > 20:
        return f'{type(r).__name__} len {len(r)} same={r is s}'
    return f'{r!r} {type(r).__name__} same={r is s}'
def t(label, f, s):
    try:
        r = f()
        out = show(r, s)
    except Exception as e:
        out = f'{type(e).__name__}: {e}'
    print(label, out)
for kn, mk in kinds():
    for cn, c in counts:
        s = mk(); t(f'{kn} * {cn}', lambda: s * c, s)
        s = mk(); t(f'{cn} * {kn}', lambda: c * s, s)
        s = mk()
        def imul1():
            x = s
            x *= c
            return x
        t(f'{kn} *= {cn}', imul1, s)
        s = mk()
        def imul2():
            x = c
            x *= s
            return x
        t(f'{cn} *= {kn}', imul2, s)
        s = mk(); t(f'mul({kn}, {cn})', lambda: operator.mul(s, c), s)
        s = mk(); t(f'imul({kn}, {cn})', lambda: operator.imul(s, c), s)
