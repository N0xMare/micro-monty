# difffuzz divergence, seed 34693
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError could not convert string to float: "0-'\\u07b8'-   0" ('could not convert string to float: "0-\'\\\\u07b8\'-   0"',)
#   mm:      caught ValueError could not convert string to float: "0-'޸'-   0" ('could not convert string to float: "0-\'޸\'-   0"',)
# signature: CPython: caught ValueError could not convert string to float: "#-'\\u#b#'-   #" ('could not convert string to | mm: caught ValueError could not convert string to float: "#-'޸'-   #" ('could not convert string to floa
import math
try:
    try:
        print(float('{}-{!r}-{:>4}'.format(divmod(round(2.5), (1 << 100) - 1)[0], chr(1976), (sum([]) * abs(42).bit_length())).strip(f'{-0.0:.3e}|{None!r} '.upper().partition(',')[2])))
    except ValueError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
