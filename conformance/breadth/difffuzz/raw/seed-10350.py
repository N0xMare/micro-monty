# difffuzz divergence, seed 10350
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError could not convert string to float: '\u0bd8' ("could not convert string to float: '\\u0bd8'",)
#   mm:      caught ValueError could not convert string to float: '௘' ("could not convert string to float: '௘'",)
# signature: CPython: caught ValueError could not convert string to float: '\u#bd#' ("could not convert string to float: ' | mm: caught ValueError could not convert string to float: '௘' ("could not convert string to float: '௘'",)
import math
try:
    def risky():
        try:
            print(chr((int('caf\u00e9'.upper()[::-1], 36) if bool(float(chr(3032))) else int('\u00df'[:2:3].join(list('mississippi'))))))
        except ValueError as e:
            print("caught", type(e).__name__, e, e.args)
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
