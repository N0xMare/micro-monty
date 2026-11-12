# difffuzz divergence, seed 5961
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '00000\u0bce'
#   mm:      EXC ValueError invalid literal for int() with base 10: '00000௎'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#bce' | mm: EXC ValueError invalid literal for int() with base #: '#௎'
import math
try:
    def f4(p0, p1, p2, *rest, kw=1):
        print({k: v for k, v in zip([str(q) for q in sorted({2, -1, 1000})], [a * b for a, b in zip([len(w) for w in p0.splitlines()], [int(p0.zfill(6))])]) if v})
    print("f4(chr(3022), list({'k0': 42, 'k1': 0, 'k2': 0, 'k3': 1000}.values()), list({'k0': 100}.values())[:-2])", f4(chr(3022), list({'k0': 42, 'k1': 0, 'k2': 0, 'k3': 1000}.values()), list({'k0': 100}.values())[:-2]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
