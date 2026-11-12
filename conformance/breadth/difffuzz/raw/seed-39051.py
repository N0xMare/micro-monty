# difffuzz divergence, seed 39051
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u2ef8'
#   mm:      EXC ValueError invalid literal for int() with base 10: '⻸'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#ef#' | mm: EXC ValueError invalid literal for int() with base #: '⻸'
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    try:
        print(undefined_name)
    finally:
        print("finally", (int('mississippi'[{'k0': 1, 'k1': 7, 'k2': 1000}['k1']]) if ('mississippi'.join([]) + ''.zfill(4)).startswith(('  pad  '[6::-1] * (sum(lst0) % 4))) else (int(chr(12024)) << (int(float('  pad  ')) % 70))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
