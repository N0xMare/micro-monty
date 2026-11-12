# difffuzz divergence, seed 30285
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u2dd7': 7}
#   mm:      {'⷗': 7}
# signature: CPython: {'\u#dd#': #} | mm: {'⷗': #}
import math
try:
    print(dict(zip(chr(11735).splitlines(), list(dict(sorted(({'k0': -17} | {'k0': 7}).items(), key=lambda kv: kv[1])).values()))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
