# difffuzz divergence, seed 17327
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ["'", '\\', 'u', '0', 'f', 'f', 'a'] '
#   mm:      ["'", '࿺'] '
# signature: CPython: ["'", '\\', 'u', '#', 'f', 'f', 'a'] ' | mm: ["'", '࿺'] '
import math
try:
    *a, b = repr(chr(4090)).rstrip()
    print(a, b)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
