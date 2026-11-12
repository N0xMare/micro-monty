# difffuzz divergence, seed 13106
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v3 {'3.5': -1, 'café': 7, '\u0fef': 123}
#   mm:      v3 {'3.5': -1, 'café': 7, '࿯': 123}
# signature: CPython: v# {'#.#': -#, 'café': #, '\u#fef': #} | mm: v# {'#.#': -#, 'café': #, '࿯': #}
import math
try:
    v3 = (dict(zip(list(dict(zip(['3.5', '\u00df', 'Hello World', '3.5'], [5]))), sorted(set([42, -1])))) | dict(sorted({'caf\u00e9'.replace('\U0001f600 ok', 'caf\u00e9'): int(7.125), chr(4079): int('123', 10)}.items(), key=lambda kv: kv[1])))
    print('v3', v3)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
