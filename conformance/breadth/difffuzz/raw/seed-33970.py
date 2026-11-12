# difffuzz divergence, seed 33970
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: 'k0\u2efdk1\u2efdk2\u2efdk3♊---'
#   mm:      EXC ValueError could not convert string to float: 'k0⻽k1⻽k2⻽k3♊---'
# signature: CPython: EXC ValueError could not convert string to float: 'k#\u#efdk#\u#efdk#\u#efdk#♊---' | mm: EXC ValueError could not convert string to float: 'k#⻽k#⻽k#⻽k#♊---'
import math
try:
    v3 = (chr(12029).join(list({'k0': 5, 'k1': 1000, 'k2': 0, 'k3': 1})) + chr(9802).ljust(4, chr(45)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    (a, b), c = tuple(sorted(({k6: len(k6) for k6 in list({'k0': 255, 'k1': 255, 'k2': 5, 'k3': 42})} | {'k0': 0, 'k1': 255}).items())), divmod(len(list(reversed([len(w) for w in []]))), int(math.exp(float(v3))))[1]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
