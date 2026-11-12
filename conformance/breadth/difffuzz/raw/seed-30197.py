# difffuzz divergence, seed 30197
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1fd5'
#   mm:      EXC ValueError invalid literal for int() with base 10: '῕'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fd#' | mm: EXC ValueError invalid literal for int() with base #: '῕'
import math
try:
    try:
        print(undefined_name)
    finally:
        print("finally", ~int(chr(8149)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
