# difffuzz divergence, seed 16054
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['b', 'u']
#   mm:      []
# signature: CPython: ['b', 'u'] | mm: []
import math
try:
    print(sorted({c for c in '{}-{!r}-{:>4}'.format(round(-2.25), chr(1970), sum(list(range(1, 12, -2)))) if c.isalpha()}))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
