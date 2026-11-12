# difffuzz divergence, seed 18159
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v10 {'a': -17, '\u20f6': 0, 'k0': 1, 'k1': 42, 'k2': 255, 'k3': 255}
#   mm:      v10 {'a': -17, '⃶': 0, 'k0': 1, 'k1': 42, 'k2': 255, 'k3': 255}
# signature: CPython: v# {'a': -#, '\u#f#': #, 'k#': #, 'k#': #, 'k#': #, 'k#': #} | mm: v# {'a': -#, '⃶': #, 'k#': #, 'k#': #, 'k#': #, 'k#': #}
import math
try:
    for i7 in range(-2, 8):
        def f8():
            n9 = 6
            while n9 > 0:
                n9 -= 1
                v10 = dict(sorted((dict(sorted(({'k0': 255} | {'k0': 1, 'k1': 42, 'k2': 255, 'k3': 255}).items(), key=lambda kv: kv[1])) | {chr(8438): (0 >> (7 % 9)), ('a' * (n9 % 4)): sum([-17])}).items(), key=lambda kv: kv[1]))
                print('v10', v10)
                def __init__(self, v, w=0):
                    self.w = w
        print('f8()', f8())
        def f12():
            def gen13(n):
                        yield k * -2
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
