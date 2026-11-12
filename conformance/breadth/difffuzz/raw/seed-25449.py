# difffuzz divergence, seed 25449
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught KeyError '\u0fe4' ('\u0fe4',)
#   mm:      caught KeyError '࿤' ('࿤',)
# signature: CPython: caught KeyError '\u#fe#' ('\u#fe#',) | mm: caught KeyError '࿤' ('࿤',)
import math
try:
    def risky():
        try:
            print(dict(sorted(({str(q): q for q in list(reversed([100, 1]))} | dict(zip(sorted([], key=lambda w: (-len(w), w)), sorted({3, 100, 2, -17})))).items(), key=lambda kv: kv[1]))[chr(4068).partition(' ')[0]])
        except Exception as e:
            print("caught", type(e).__name__, e, e.args)
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
