# difffuzz divergence, seed 9008
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC IndexError list index out of range
#   mm:      EXC MemoryError 
# signature: CPython: EXC IndexError list index out of range | mm: EXC MemoryError 
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    for i15 in (list(map(lambda q: q * -2 + 1, [-3, 100, -3, 42])) * 3):
        data = [(1, "b"), (0, "c"), (1, "a"), (-2, "z")]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class C16:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C16(((2 ** 64 ** (~(i15 >> (0 % 9)) % 7)) >> (((int(7.125) ^ lst0[-17]) & len({1, 7, 1, 2})) % 9)), 3), C16(divmod(int(-(3.0 ** 2)), f9())[0], -2), C16((math.trunc(abs(math.floor(abs(pow(7, 19, 2 ** 61 - 1)) % 30)) % 30) >> (((int('') << (+1 % 70)) ** (int(round(-2.25, 2)) % 7)) % 9)), -3), C16.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
