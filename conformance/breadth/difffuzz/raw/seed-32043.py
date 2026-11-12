# difffuzz divergence, seed 32043
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: 0-'\u191f😀\u191fo\u191f'-
#   mm:      0-'᤟😀᤟o᤟'-
# signature: CPython: #-'\u#f😀\u#fo\u#f'- | mm: #-'᤟😀᤟o᤟'-
import math
try:
    v6 = chr(6431)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print('{}-{!r}-{:>4}'.format(math.trunc(abs(1e-05)), '\U0001f600 ok'[::2].replace('mississippi'.split('s')[-2], v6.replace(v6, v6)), max([len(w) for w in ['\U0001f600 ok', 'mississippi', 'Hello World', 'x\ny']])).partition(' ')[0])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
