# difffuzz divergence, seed 20798
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u0cd8'
#   mm:      EXC ValueError invalid literal for int() with base 16: '೘'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#cd#' | mm: EXC ValueError invalid literal for int() with base #: '೘'
import math
try:
    def f1(p0, p1):
        class Ctx2:
            def __init__(self, name):
                self.name = name
            def __enter__(self):
                return self
            def __exit__(self, kind, value, tb):
                return None
        with Ctx2("a") as x, Ctx2("b"):
            pass
        return int(chr(3288), 16)
    print("f1(('caf\\u00e9' + chr(6093).strip()), chr(396))", f1(('caf\u00e9' + chr(6093).strip()), chr(396)))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
