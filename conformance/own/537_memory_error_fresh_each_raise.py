# A MemoryError for a size too big for memory is a fresh MemoryError() each
# time, with empty args, as CPython's PyErr_NoMemory: two saved errors are
# two objects, each with its own attributes and traceback, and one raised
# while handling another exception has no __context__.
import sys

big = sys.maxsize // 2 + 1
errs = []


def g(i):
    if i:
        return (1, 2) * big
    return [1, 2] * big


for i in range(3):
    try:
        g(i % 2)
    except MemoryError as e:
        e.tag = i
        errs.append(e)
print(errs[0] is errs[1], errs[1] is errs[2], [e.tag for e in errs])
print([repr(e) for e in errs], [e.args for e in errs], str(errs[0]) == '')
print([e.__traceback__.tb_next.tb_lineno for e in errs])
print(MemoryError().args, MemoryError('').args)
try:
    {}['k']
except KeyError:
    try:
        bytearray(b'ab') * big
    except MemoryError as e:
        print(e.__context__, e.__cause__, e.__suppress_context__)
try:
    try:
        [0] * big
    except MemoryError as e:
        f = e
        raise
except MemoryError as e2:
    print(e2 is f)
try:
    try:
        [0] * big
    except MemoryError as e:
        f = e
        raise e from KeyError('k')
except MemoryError as e2:
    print(e2 is f, repr(e2.__cause__))
