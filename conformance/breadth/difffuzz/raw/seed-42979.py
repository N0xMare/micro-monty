# difffuzz divergence, seed 42979
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: v13 {'\u0c11': 3}
#   mm:      v13 {'఑': 3}
# signature: CPython: v# {'\u#c#': #} | mm: v# {'఑': #}
import math
try:
    def f12():
        v13 = dict(zip(list(chr(3089)), sorted([len(w) for w in sorted(['123', '-42', 'AbC'], key=str.lower)], key=str)))
        print('v13', v13)
        match (1, "x"):
            case [x]:
                print("one", x)
        def rec14(n):
            if n <= 1:
                return n
    print('f12()', f12())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
