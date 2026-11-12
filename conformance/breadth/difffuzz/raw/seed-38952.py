# difffuzz divergence, seed 38952
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u07bc'
#   mm:      EXC ValueError invalid literal for int() with base 36: '޼'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#bc' | mm: EXC ValueError invalid literal for int() with base #: '޼'
import math
try:
    n11 = 1
    while n11 > 0:
        n11 -= 1
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    if all(([] * 3)):
        v12 = n11
        v12 += sorted(list(filter(lambda q: q > 3, [5, 5, 10, -1]))[7:], key=str)[(divmod(sum([3, 10, 10, 100, 0]), {'k0': -1}['k0'])[1] >> (int(chr(1980), 36) % 9))]
        for i13 in range(-3, 8):
            if i13 == 4:
                continue
            try:
                print([1, 2].remove(abs(divmod(~(0 ** (n11 % 7)), (v8['k1'] << (10 ** 30 + 7 % 70)))[0]).bit_length()))
            except Exception as e:
                print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
