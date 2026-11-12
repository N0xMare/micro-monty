# difffuzz divergence, seed 14206
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v6 (("3-'1'-   3", 0), ('\u0ffa', 147573952589676412928000), ('ῦ', 0))
#   mm:      v6 (("3-'1'-   3", 0), ('࿺', 147573952589676412928000), ('ῦ', 0))
# signature: CPython: v# (("#-'#'-   #", #), ('\u#ffa', #), ('ῦ', #)) | mm: v# (("#-'#'-   #", #), ('࿺', #), ('ῦ', #))
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v6 = tuple(sorted({chr(8166).split('s')[-1]: ~''.rfind('-42'), '{}-{!r}-{:>4}'.format(math.floor(3.0), '123'[-3], max(lst0)): ('' * (-3 % 4)).count(repr({})), chr(4090): +(1000 << (-3 % 70))}.items()))
    print('v6', v6)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
