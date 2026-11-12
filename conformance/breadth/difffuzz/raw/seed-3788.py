# difffuzz divergence, seed 3788
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: "5.0|'\\u2e75'"
#   mm:      EXC ValueError invalid literal for int() with base 10: "5.0|'⹵'"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "#.#|'\\u#e#'" | mm: EXC ValueError invalid literal for int() with base #: "#.#|'⹵'"
import math
try:
    class C4:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C4(int('%s|%r' % (float(min([5, 10, 42])), chr(11893).expandtabs(),)), 2), C4((pow(pow((-17 if False else 2), 23, 7), 23, -5) if (5 in tuple(list(range(0, 7, -2)))) else math.factorial(abs(round((v3 + 7.125))) % 30)), 0), C4(math.factorial(abs((abs((-1 >> (2 % 9))).bit_length() ** (0 % 7))) % 30), -4), C4.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
