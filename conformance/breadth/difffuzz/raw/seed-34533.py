# difffuzz divergence, seed 34533
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u12d7---'
#   mm:      EXC ValueError could not convert string to float: '዗---'
# signature: CPython: EXC ValueError could not convert string to float: '\u#d#---' | mm: EXC ValueError could not convert string to float: '዗---'
import math
try:
    v2 = 'a'.join(list({'Hello World'[10]: 5}))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    try:
        print(float(chr(4823).lstrip(repr({'k0': 2, 'k1': 42})).ljust(4, chr(45)).swapcase()))
    finally:
        print("finally", [ord(c) for c in '{}-{!r}-{:>4}'.format(5, '\U0001f600 ok', 5).replace(v2.lstrip('a,b,,c'), (v2 + v2))][~divmod(v2.find(v2), -17)[0]])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
