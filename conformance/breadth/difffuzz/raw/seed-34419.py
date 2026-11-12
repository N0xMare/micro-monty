# difffuzz divergence, seed 34419
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: [,',\,u,2,e,f,9,',],+,0,0,0,,,0,0,0,|
#   mm:      [,',⻹,',],+,0,0,0,,,0,0,0,|
# signature: CPython: [,',\,u,#,e,f,#,',],+,#,#,#,,,#,#,#,| | mm: [,',⻹,',],+,#,#,#,,,#,#,#,|
import math
try:
        print(*list(reversed(list(range(-3, -5, 3)))), *(repr(chr(12025).split()) + f'{pow(100, 24, -5):+08,}|'), sep=",")
        def gen14(n):
            for k in range(n):
                if k % 2 == 0:
                    yield k * 3
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
