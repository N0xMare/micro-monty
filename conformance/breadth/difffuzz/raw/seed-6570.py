# difffuzz divergence, seed 6570
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u0f98'
#   mm:      EXC ValueError invalid literal for int() with base 10: '྘'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#f#' | mm: EXC ValueError invalid literal for int() with base #: '྘'
import math
try:
    v16 = chr(3992).partition(' ')[0]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f21(p0, p1):
        return sum([(round(1e300) + i24) for i24 in range(6)])
    print("f21(((list({'k0': 7, 'k1': 2, 'k2': -1}.values()) * 1)[round(abs(-2.25))] ** (divmod(abs(int(v16)).bit_length(), ((n12 >> (5 % 9)) // int('tab\\there', 2)))[1] % 7)), v16.partition(' ')[1].replace(v16.title(), f'{1e-05:.3e}|').rjust(6, chr(45)).find('{}-{!r}-{:>4}'.format(abs(-3).bit_length(), ('caf\\u00e9' * (1 % 4)), [1][n12])[:]))", f21(((list({'k0': 7, 'k1': 2, 'k2': -1}.values()) * 1)[round(abs(-2.25))] ** (divmod(abs(int(v16)).bit_length(), ((n12 >> (5 % 9)) // int('tab\there', 2)))[1] % 7)), v16.partition(' ')[1].replace(v16.title(), f'{1e-05:.3e}|').rjust(6, chr(45)).find('{}-{!r}-{:>4}'.format(abs(-3).bit_length(), ('caf\u00e9' * (1 % 4)), [1][n12])[:])))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
