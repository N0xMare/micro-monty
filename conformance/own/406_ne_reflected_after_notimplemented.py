# `a != b` where a's class has `__eq__` but no `__ne__`: when the inverted
# `__eq__` returns NotImplemented, b's reflected `__ne__` runs next (it was
# skipped, and identity decided). datetime types against an always-equal
# object and an object that orders above everything.
class AlwaysEq:
    def __eq__(self, o): return True
    def __ne__(self, o): return False
class X:
    def __eq__(self, o): return NotImplemented
class Y:
    def __eq__(self, o):
        if not isinstance(o, Y): return NotImplemented
        return True
A = AlwaysEq()
print(1 != A, object() != A, X() != A, Y() != A, Y() == A, A != Y())
from datetime import date, datetime, timedelta, time
class AlwaysEq:
    def __eq__(self, o): return True
    def __ne__(self, o): return False
class Largest:
    def __eq__(self, o): return isinstance(o, Largest)
    def __lt__(self, o): return False
    def __gt__(self, o): return True
    def __le__(self, o): return isinstance(o, Largest)
    def __ge__(self, o): return True
A, L = AlwaysEq(), Largest()
for me in [date(1, 1, 1), datetime(1, 1, 1), timedelta(1, 1, 1), time(1, 1, 1)]:
    print(type(me).__name__, me == A, me != A, me < L, me > L, me <= L, me >= L, me == (), () != me)
