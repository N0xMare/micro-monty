# difffuzz divergence, seed 19990
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1ad6'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᫖'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ad#' | mm: EXC ValueError invalid literal for int() with base #: '᫖'
import math
try:
    print(*([] + [i + v for i, v in enumerate([(divmod(42, 2)[1] + i4) for i4 in range(-3, 7) if i4 % 3], 0)]), *(chr(3932) + ('tab\there' * (1 % 4)).capitalize()[int(chr(6870))]), sep=",")
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
