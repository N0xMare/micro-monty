# difffuzz divergence, seed 30850
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0fec'
#   mm:      EXC ValueError invalid literal for int() with base 16: '࿬'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fec' | mm: EXC ValueError invalid literal for int() with base #: '࿬'
import math
try:
    g = (q * 2 for q in [(int(chr(4076), 16) if ('-42'.rfind('hello') < round(-0.0) <= +100) else (len({'k0': 42}) >> (round(1.5) % 9))), min(([ord(c) for c in 'caf\u00e9'] + [a * b for a, b in zip([100, 1, 1, 255], [10])])), 42])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
