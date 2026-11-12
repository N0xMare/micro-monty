# difffuzz divergence, seed 11243
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 16: '\u1289'
#   mm:      EXC ValueError invalid literal for int() with base 16: '኉'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#' | mm: EXC ValueError invalid literal for int() with base #: '኉'
import math
try:
    for i14 in range(2, 7):
        def rec15(n):
            if n <= 1:
                return n
                for k in range(n):
                        yield k * 1
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f23():
        if (not isinstance([0, 100][:][7:], int)):
            lst0 = [3, 1, 2]
            lst0.insert(1, int(chr(4745).strip(f'{1e-05:G}{i14:+} and {lst0!r} '), 16))
            def f26():
                return repr(((-2.25 ** 2) - abs(0.0))).ljust(6, chr(45))
    print('f23()', f23())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
