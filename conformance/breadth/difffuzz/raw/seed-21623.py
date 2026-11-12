# difffuzz divergence, seed 21623
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught KeyError '\u0d97' ('\u0d97',)
#   mm:      caught KeyError '඗' ('඗',)
# signature: CPython: caught KeyError '\u#d#' ('\u#d#',) | mm: caught KeyError '඗' ('඗',)
import math
try:
        class Ctx4:
            def __init__(self, name):
                self.name = name
            def __enter__(self):
                return self
            def __exit__(self, kind, value, tb):
                return True
        with Ctx4("a") as x, Ctx4("b"):
            print(1 / 0)
        v5 = list(range(5, 9, -2))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    try:
        print(dict(zip(sorted(sorted(['caf\u00e9', 'Hello World', '-42', 'AbC'], key=lambda w: w[::-1]), key=lambda w: (-len(w), w)), [i + v for i, v in enumerate(v5, 2)]))[chr(3479)])
    except Exception as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
