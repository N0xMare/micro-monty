# A byte argument (`_getbytevalue`, `Vm::byte_arg`) on its int path and
# off it: an int subclass and an IntEnum are their values, an `__index__`
# object goes through it, and every other value is refused with CPython's
# message, through `byte_arg`'s four callers: `append`, `insert`, `remove`
# and item assignment with an int key. The int path is `byte_arg`'s; the
# rest is its cold `byte_arg_other`. The `in` and `find` lines exercise
# the neighbouring paths, `bytes_in` and `needle`, which read the byte
# themselves. Left out: an `__index__` object as the needle of `in`,
# `find`, `count`, `index` and their kin, which CPython takes and mm
# refuses with TypeError [M].
from enum import IntEnum


class E(IntEnum):
    A = 65


class I(int):
    pass


class X:
    def __index__(self):
        return 66


b = bytearray()
b.append(E.A)
b.append(I(67))
b.append(True)
b.append(X())
b.insert(0, I(70))
b.insert(-1, E.A)
b.insert(1, X())
b[1] = I(71)
b[-1] = E.A
b[0] = X()
print(b, I(65) in b, E.A in b, b.find(I(71)), b.find(E.A))
b.remove(E.A)
b.remove(I(71))
b.remove(X())
print(b)

BAD = [1.5, 'a', None, 256, -1, I(300), 2**70, E.A * 4]


def store(v):
    b[0] = v


def show(name, f):
    try:
        f()
        print(name, 'ok', b)
    except (TypeError, ValueError, OverflowError) as e:
        print(name, type(e).__name__, e)


for bad in BAD:
    show('append', lambda: b.append(bad))
    show('insert', lambda: b.insert(0, bad))
    show('setitem', lambda: store(bad))
    show('remove', lambda: b.remove(bad))
    show('in', lambda: bad in b)
    show('find', lambda: b.find(bad))
print(b)
