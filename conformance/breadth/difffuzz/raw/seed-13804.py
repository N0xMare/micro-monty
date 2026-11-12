# difffuzz divergence, seed 13804
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 10: '+0.00 😀 okAbC\u10c6 42    |' ("invalid literal for int() with base 10: '+0.00 😀 okAbC\\u10c6 42    |'",)
#   mm:      caught ValueError invalid literal for int() with base 10: '+0.00 😀 okAbC჆ 42    |' ("invalid literal for int() with base 10: '+0.00 😀 okAbC჆ 42    |'",)
# signature: CPython: caught ValueError invalid literal for int() with base #: '+#.# 😀 okAbC\u#c# #    |' ("invalid litera | mm: caught ValueError invalid literal for int() with base #: '+#.# 😀 okAbC჆ #    |' ("invalid literal fo
import math
try:
    v1 = (('\U0001f600 ok'.partition(',')[0] + ('AbC' * (5 % 4))) + chr(4294))[:]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    for i4 in range(-3, 6):
        if i4 == 3:
            break
        def gen5(n):
            for k in range(n):
                    yield k * 1
        it = gen5(1)
        n6 = 4
        while n6 > 0:
            n6 -= 1
            n7 = 0
            while n7 > 0:
                print(lst0)
            data = [(1, "b"), (0, "c"), (1, "a"), (-2, "z")]
            def gen8(n):
                for k in range(n):
                        yield k * -1
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    try:
        print(v1.partition(',')[2].expandtabs().zfill(3).split(None)[0] + int(f'{float(n7):+.2f} {v1!s} {(42 << (n6 % 70)):<6}|'))
    except (TypeError, ValueError) as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
