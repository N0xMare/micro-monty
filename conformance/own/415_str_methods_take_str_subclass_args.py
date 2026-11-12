# str methods and `in` take a str subclass (a StrEnum member) as their
# argument, as its value: startswith/endswith, replace, split/rsplit, find,
# count, index, partition (which returns the argument itself), strip,
# removeprefix, ljust's fill. Error messages name find()/count()/index().
from enum import StrEnum
class C(StrEnum):
    A = 'a'
    B = 'b,c'
s = 'xa b,c a'
tests = [
    lambda: s.startswith(C.A), lambda: s.endswith((C.A, 'z')), lambda: s.replace(C.A, 'Q'),
    lambda: s.split(C.B), lambda: s.find(C.A), lambda: s.count(C.A), lambda: C.A in s,
    lambda: s.index(C.B), lambda: s.partition(C.B), lambda: s.rpartition(C.A), lambda: s.partition('q'),
    lambda: s.strip(C.A), lambda: s.rsplit(C.A, 1), lambda: s.removeprefix(C.A[0]),
    lambda: 'abc'.translate(str.maketrans(C.A, 'z')), lambda: s.lstrip(C.A), lambda: 'xx'.ljust(4, C.A),
    lambda: 'xx'.center(6, C.A), lambda: s.rfind(C.A), lambda: s.rindex(C.A), lambda: C.B in C.B,
    lambda: s.count(C.A, 2), lambda: s.find(C.A, 3), lambda: s.endswith(C.A, 0, 2),
]
for i, t in enumerate(tests):
    try:
        r = t()
    except Exception as e:
        r = f'{type(e).__name__}: {e}'
    print(i, repr(r))
status = C.A
if status in 'banana' and 'banana'.startswith('b'):
    print('found', 'banana'.count(status))
for f in (lambda: 'abc'.find(1), lambda: 'abc'.count(None), lambda: 'abc'.partition(2),
          lambda: 'abc'.index([]), lambda: 'a'.startswith(1), lambda: 1 in 'a', lambda: 'a'.replace(1, 'x')):
    try:
        print(repr(f()))
    except TypeError as e:
        print(e)
