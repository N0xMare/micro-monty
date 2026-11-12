# difffuzz divergence, seed 15719
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u085d': 3}
#   mm:      {'࡝': 3}
# signature: CPython: {'\u#d': #} | mm: {'࡝': #}
import math
try:
    v8 = (([i + v for i, v in enumerate(list(reversed([1000, -1])), 2)] + ([1, 10] + [1000, 1, 1000][-5::3])) + sorted([], key=str)[:])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print({k: v for k, v in zip(chr(2141).splitlines(), [a * b for a, b in zip(list(filter(lambda q: q > 1, [i + v for i, v in enumerate(v8, 2)][:1:2])), [len(w) for w in [str(q) for q in (v8 * 1)]])]) if v})
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
