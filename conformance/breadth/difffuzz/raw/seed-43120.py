# difffuzz divergence, seed 43120
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: 'AbC\u0ebe'
#   mm:      EXC ValueError invalid literal for int() with base 36: 'AbC຾'
# signature: CPython: EXC ValueError invalid literal for int() with base #: 'AbC\u#ebe' | mm: EXC ValueError invalid literal for int() with base #: 'AbC຾'
import math
try:
    class C10:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C10(int(('AbC' + chr(3774).lstrip()), 36), 4), C10(list(range(3, 5, 1))[round(float(v1.replace('tab\there', v1)))], -4), C10(divmod((v2['k0'] | ~abs(i3).bit_length()), ((max([42]) << ((3 if False else i3) % 70)) << (([0] * 0)[int(0.1)] % 70)))[0], 3), C10.make(0)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
