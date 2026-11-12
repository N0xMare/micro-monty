# difffuzz divergence, seed 15434
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '00000\u1aeb'
#   mm:      EXC ValueError invalid literal for int() with base 0: '00000᫫'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '#\u#aeb' | mm: EXC ValueError invalid literal for int() with base #: '#᫫'
import math
try:
    v1 = 2
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v6 = chr(6891).zfill(6)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    try:
        print((tuple(sorted({'\U0001f600 ok': v1, v6: v1}.items())) + tuple(sorted({100, 1000, 7})))[2:][max([(+int(v6, 0) + i8) for i8 in range(4) if i8 % 2])])
    except KeyError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
