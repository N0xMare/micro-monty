# difffuzz divergence, seed 6832
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u18fc'
#   mm:      EXC ValueError invalid literal for int() with base 36: '᣼'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fc' | mm: EXC ValueError invalid literal for int() with base #: '᣼'
import math
try:
    if isinstance(int(chr(6396), 36), bool):
        match f'{[0, 3, 5, 10, 255]} '.strip().title():
            case {}:
                print("any mapping")
        for i7 in range(1, 4):
            def rec8(n):
                if n <= 1:
                    return n
        try:
            print(float('3.5'.swapcase()))
        finally:
            print("finally", (divmod(~len({'k0': 100}), {}['k2'])[0] ^ len('  pad  ')))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
