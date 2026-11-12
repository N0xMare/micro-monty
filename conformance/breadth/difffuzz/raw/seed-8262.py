# difffuzz divergence, seed 8262
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: 42-'\u2cf7'-  -3
#   mm:      42-'⳷'-  -3
# signature: CPython: #-'\u#cf#'-  -# | mm: #-'⳷'-  -#
import math
try:
    print('{}-{!r}-{:>4}'.format(42, chr(11511).capitalize(), -(42 if False else 3)).zfill(8))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
