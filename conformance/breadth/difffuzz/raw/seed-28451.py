# difffuzz divergence, seed 28451
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC KeyError 'k0'
#   mm:      EXC MemoryError 
# signature: CPython: EXC KeyError 'k#' | mm: EXC MemoryError 
import math
try:
    v3 = [a * b for a, b in zip(sorted(set(sorted([10], key=lambda q: (q % 3, -q)))), [i + v for i, v in enumerate(sorted([-17, 0, 42])[:1], 1)])]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v4 = ((2 ** 64 ** (max(list(reversed([5, 2]))) % 7)) >> (({k5: len(k5) for k5 in ['123']} | dict(zip(['a,b,,c'], v3)))['k0'] % 9))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
