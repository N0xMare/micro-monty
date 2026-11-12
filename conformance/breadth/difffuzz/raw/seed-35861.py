# difffuzz divergence, seed 35861
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u12b7': 3}
#   mm:      {'኷': 3}
# signature: CPython: {'\u#b#': #} | mm: {'኷': #}
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    n4 = 1
    while n4 > 0:
        def gen5(n):
            for k in range(n):
                if k % 2 == 0:
                    yield k * 2
        it = gen5(5)
        def gen6(n):
            for k in range(n):
                if k % 3 == 0:
                    yield k * 2
        n7 = 2
        while n7 > 0:
            print({k: v for k, v in zip(chr(4791).partition(',')[0].splitlines(), [a * b for a, b in zip(lst0, sorted(list(map(lambda q: q * 0 + 1, list(map(lambda q: q * 3 + 1, lst0)))), key=abs))]) if v})
            class Err9(Exception):
                    self.code = code
            try:
                try:
                    raise Err9('tab\there'.partition(',')[1].zfill(0)[:].expandtabs(), 7)
                except Err9 as inner:
                    raise RuntimeError("wrapped") from inner
            except RuntimeError as outer:
                print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
                def __init__(self, name):
                    self.name = name
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
