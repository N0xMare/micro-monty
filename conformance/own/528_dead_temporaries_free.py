# A value an expression leaves in a register is dead once its statement
# ends: after `gc.collect()` its memory is free again. Each shape holds
# 100 MB that no name binds, then allocates 200 MB; under the 256 MiB
# limit the second fits only if the first was freed (in 50 MB blocks,
# under the object size cap).
import gc

N = 50_000_000


class H:
    def __init__(self):
        self.d = [bytearray(N), bytearray(N)]

    def __repr__(self):
        return 'H'

    def __format__(self, spec):
        return 'H'

    def __lt__(self, other):
        return False

    def __gt__(self, other):
        return True


def hold():
    return H()


def four(a, b, c, d):
    return 4


def kw(a, b, c, *, d):
    return 5


def keep(x):
    y = x
    z = y
    return 1


def fits(tag):
    gc.collect()
    try:
        b = [bytearray(N) for _ in range(4)]
        print(tag, 'ok', len(b))
    except MemoryError:
        print(tag, 'MemoryError')


def user_call():
    n = four('t', 0, 0, hold())
    fits('fourth argument')


def keyword_call():
    n = kw(0, 0, 0, d=hold())
    fits('keyword argument')


def callee_locals():
    n = keep(hold())
    fits("a callee's locals")


def to_print():
    print(0, 0, 0, hold())
    fits('print')


def to_max():
    m = max(0, 0, 0, hold())
    m = None
    fits('max')


def list_display():
    n = len([0, 0, 0, hold()])
    fits('list display')


def tuple_display():
    n = len((0, 0, 0, hold()))
    fits('tuple display')


def dict_display():
    n = len({0: 0, 1: 1, 2: hold()})
    fits('dict display')


def fstring():
    s = f'{0}{0}{0}{hold()}'
    fits('f-string')


def method_args():
    xs = []
    xs.append((0, 0, hold()))
    xs.clear()
    fits('method arguments')


def then_aug():
    x = 0
    n = four('t', 0, 0, hold())
    x += 1
    fits('then x += 1')


def loop_rounds():
    for i in range(4):
        e = hold()
        print(i, 'ok', len(e.d))
        e = None
    fits('loop')


def loop_body():
    for i in [1, 2, 3]:
        n = four('t', 0, 0, hold())
    fits('loop body')


def if_test():
    if four('t', 0, 0, hold()):
        fits('if body')


def generator_expression():
    print(sum(x for x in [four(0, 0, 0, hold())]))
    fits('generator expression')


user_call()
keyword_call()
callee_locals()
to_print()
to_max()
list_display()
tuple_display()
dict_display()
fstring()
method_args()
then_aug()
loop_rounds()
loop_body()
if_test()
generator_expression()
n = four('t', 0, 0, hold())
fits('module level')
