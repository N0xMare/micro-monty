# difffuzz divergence, seed 13189
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u1289'
#   mm:      EXC ValueError invalid literal for int() with base 10: '኉'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '኉'
import math
try:
    try:
        print(((list(reversed([-1, 1]))[(-3 >> (1000 % 9))] if repr(['\U0001f600 ok', 'caf\u00e9', 'AbC', 'mississippi']).isdigit() else -(10 ** 25)) >> ((int(chr(4745)) * {'3.5': 3, 'a,b,,c': 100}['k0']) % 9)).nope)
    except IndexError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
