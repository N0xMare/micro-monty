# difffuzz divergence, seed 8726
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v24 ('***\u171b', 3)
#   mm:      v24 ('***᜛', 3)
# signature: CPython: v# ('***\u#b', #) | mm: v# ('***᜛', #)
import math
try:
    v2 = repr(list('3.5'.swapcase()))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    n23 = 6
    while n23 > 0:
        n23 -= 1
        lst0 = [3, 1, 2]
        v24 = (chr(5915).rjust(1, chr(42)).rjust(4, chr(42)), abs((round(1e300) if v2.startswith('AbC') else sum([-1, 7]))).bit_length(), )
        print('v24', v24)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
