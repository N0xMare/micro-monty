# difffuzz divergence, seed 9182
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1ad9'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᫙'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ad#' | mm: EXC ValueError invalid literal for int() with base #: '᫙'
import math
try:
    match ([a * b for a, b in zip([i + v for i, v in enumerate([5], 1)], ([-17] + []))][len(({'k0': -17, 'k1': 2} | {'k0': 1}))] if isinstance(math.trunc(abs(min([5, 0])) % 30), bool) else int(chr(6873))):
        case str(s) if len(s) > 3:
            print("long", s[:3])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
