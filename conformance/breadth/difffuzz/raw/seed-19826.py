# difffuzz divergence, seed 19826
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: 'AbC3.5AbCa\u1ae1'
#   mm:      EXC ValueError invalid literal for int() with base 10: 'AbC3.5AbCa᫡'
# signature: CPython: EXC ValueError invalid literal for int() with base #: 'AbC#.#AbCa\u#ae#' | mm: EXC ValueError invalid literal for int() with base #: 'AbC#.#AbCa᫡'
import math
try:
    n8 = 3
    while n8 > 0:
        for i9 in range(2):
            class Ctx10:
                def __init__(self, name):
                    self.name = name
                raise ValueError(7)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    try:
        print(math.sqrt((int(('AbC'.join(['', '3.5', 'a']) + chr(6881))) / int('%x' % (max(lst0),), 10))))
    except ArithmeticError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
