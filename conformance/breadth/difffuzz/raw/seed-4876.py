# difffuzz divergence, seed 4876
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0ff8'
#   mm:      EXC ValueError invalid literal for int() with base 10: '࿸'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ff#' | mm: EXC ValueError invalid literal for int() with base #: '࿸'
import math
try:
    try:
        print(divmod('  pad  '.rfind('3.5'), int(round(math.tau, -1)))[int(chr(4088))])
    except IndexError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
