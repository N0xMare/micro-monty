# `math.trunc` uses only the class's `__trunc__`: an instance attribute,
# `__float__`, `__index__` and `__int__` do not stand in for it, and a type
# without one gets CPython's TypeError.
import math
def t(tag, f):
    try:
        print(tag, f())
    except Exception as e:
        print(tag, type(e).__name__, e)
class M: pass
m = M()
m.__trunc__ = lambda: 3
class F:
    def __float__(self): return 2.5
    def __index__(self): return 2
    def __int__(self): return 2
class T:
    def __trunc__(self): return 'any'
class I(int): pass
class Fl(float): pass
class IT(int):
    def __trunc__(self): return 'it'
class S(str): pass
class N:
    __trunc__ = None
for tag, v in [('instance attr', m), ('none', M()), ('float/index only', F()), ('class', T()),
               ('int sub', I(7)), ('float sub', Fl(7.9)), ('int sub trunc', IT(7)),
               ('str sub', S('1')), ('None attr', N()),
               ('str', 'a'), ('None', None), ('complex', 1j), ('list', []), ('bool', True),
               ('nan', float('nan')), ('inf', float('inf')), ('neg', -2.7), ('big', 1e30),
               ('type', M)]:
    t(tag, lambda: math.trunc(v))
