# difffuzz divergence, seed 28214
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v10 {'\u0b34': 1}
#   mm:      v10 {'଴': 1}
# signature: CPython: v# {'\u#b#': #} | mm: v# {'଴': #}
import math
try:
    for i4 in list(range(-2, -1, 3)):
        match repr(sorted(list('Hello World'), key=lambda w: (-len(w), w))).partition(' ')[2]:
            case 0 | 1 | 2:
                print("small")
        v5 = (('tab\there'.join(['tab\there', 'mississippi']) * ({'k0': -1, 'k1': -3, 'k2': -1, 'k3': -17}['k0'] % 4)) + ('tab\there' * (i4 % 4)).partition('s')[1]).split('s')[-1]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v10 = {k11: len(k11) for k11 in (chr(2868) + v5.removeprefix(v5)).split()}
    print('v10', v10)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
