# difffuzz divergence, seed 36525
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v14 ['\u0be3']
#   mm:      v14 ['௣']
# signature: CPython: v# ['\u#be#'] | mm: v# ['௣']
import math
try:
    def f26():
        x = y = [len(w) for w in sorted('AbC'.partition(',')[0].split(','), key=len)]
        def gen29(n):
            for k in range(n):
                if k % 3 == 0:
                    yield k * 0
        it = gen29(1)
        v14 = chr(3043).zfill(1).splitlines()
        print('v14', v14)
    print('f26()', f26())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
