# difffuzz divergence, seed 350
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 2: '  pad  000\u124ecafé'
#   mm:      EXC ValueError invalid literal for int() with base 2: '  pad  000቎café'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '  pad  #\u#ecafé' | mm: EXC ValueError invalid literal for int() with base #: '  pad  #቎café'
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    for i58 in sorted(lst0, key=abs):
        match tuple([i + v for i, v in enumerate([a * b for a, b in zip([a * b for a, b in zip(lst0, lst0)], list(range(1, 10, 2)))], 1)]):
            case (a, "x"):
                print("pair", a)
        print(int(chr(4686).zfill(4).join(list({k60: len(k60) for k60 in ['  pad  ', 'caf\u00e9']})), 2), end='caf\u00e9')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
