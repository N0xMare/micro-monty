# A register holds nothing for a collection until its statement stores to
# it: what an earlier statement left there is free as soon as memory is
# needed, with no `gc.collect()` between the two. Each shape makes 150 MB
# twice under the 256 MiB limit (in 50 MB blocks, under the object size
# cap) and binds neither to a name; the second fits only if the first is
# not a root while the second is made. python3.14 prints "ok" for each.

N = 50_000_000


def load():
    return [bytearray(N) for _ in range(3)]


def four(a, b, c, data):
    return len(data)


class Box:
    @property
    def data(self):
        return load()

    @property
    def size(self):
        return len(load())

    def count(self, data):
        return len(data)


def argument():
    # The slot of a call's argument, before the argument is stored.
    n = len(load())
    m = len(load())
    return n + m


def fourth_argument():
    n = four('t', 0, 0, load())
    m = four('u', 0, 0, load())
    return n + m


def loop():
    t = 0
    for i in range(3):
        t += len(load())
    return t


def display():
    n = len([0, 1, 2, load()][3])
    m = len([0, 1, 2, load()][3])
    return n + m


def method():
    box = Box()
    rows = []
    rows.append(box.count(load()))
    rows.append(box.count(load()))
    return rows


def reserved():
    # The register a call window keeps for the callee's frame header: the
    # caller never stores to it.
    (0, load())
    n = len(load())
    return n


def above():
    # A register past the ones the statement has allocated so far, which
    # it allocates later (the second call's window).
    (0, 0, 0, 0, 0, load())
    n = len(load()) + four(0, 0, 0, '....')
    return n


def range_arguments():
    # The registers of a `for ... in range(...)`, while its argument is
    # evaluated.
    (0, 0, 0, load())
    t = 0
    for i in range(len(load())):
        t += i
    return t


def descriptor():
    # An argument that runs Python code from another instruction than a
    # call: the property's frame sits above every register of this one.
    box = Box()
    n = len(box.data)
    m = len(box.data)
    return n + m


def operand():
    # A binary operator's left operand, stored after its code has run.
    box = Box()
    (load(), 0)
    n = box.size + 1
    return n


def triple():
    return len(load()), 1, 2


def unpacking():
    # The registers an unpacking assignment's targets take once its value
    # is there.
    (0, 0, load())
    a, b, c = triple()
    return a + b + c


def subscript():
    # A subscript's index, evaluated after the object.
    d = {3: 'three'}
    (0, load())
    d[len(load())] = 'again'
    return sorted(d.items())


def first(data):
    return [len(data)]


def spent_argument(range=first):
    # The argument of a `range(...)` call that is not the builtin's, once
    # the call has returned: the loop keeps its counters where the call's
    # window was.
    t = 0
    for i in range(load()):
        t += i + len(load())
    return t


def alternative():
    # A capture's register while a later alternative of an or-pattern
    # runs: the one before it stored to it, or failed first.
    box = Box()
    (0, load())
    match box:
        case [a] | Box(size=a):
            return a


SHAPES = [
    argument, fourth_argument, loop, display, method, reserved, above,
    range_arguments, descriptor, operand, unpacking, subscript,
    spent_argument, alternative,
]
for shape in SHAPES:
    try:
        print(shape.__name__, 'ok', shape())
    except MemoryError:
        print(shape.__name__, 'MemoryError: the first 150 MB is still held')

# At module level, where the registers are the module frame's.
try:
    n = len(load())
    m = len(load())
    print('module ok', n + m)
except MemoryError:
    print('module MemoryError: the first 150 MB is still held')
