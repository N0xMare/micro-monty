# difffuzz divergence, seed 2284
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: "Hello World\u13ffHello World['a,b,,c', 'a,b,,c', 'a', '-42']"
#   mm:      EXC ValueError invalid literal for int() with base 10: "Hello World᏿Hello World['a,b,,c', 'a,b,,c', 'a', '-42']"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "Hello World\u#ffHello World['a,b,,c', 'a,b,,c | mm: EXC ValueError invalid literal for int() with base #: "Hello World᏿Hello World['a,b,,c', 'a,b,,c', '
import math
try:
    v2 = chr(5119).replace('', 'Hello World'.title().rjust(11, chr(45)).zfill(1))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def risky():
        try:
            print(int((v2 + repr(['a,b,,c', 'a,b,,c', 'a', '-42']))))
        except LookupError as e:
            print("caught", type(e).__name__, e, e.args)
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
