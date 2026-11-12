# difffuzz divergence, seed 693
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['0', '1000']helloEXC ValueError could not convert string to float: 'k0\u0c11--------k1\u0c11--------k2'
#   mm:      ['0', '1000']helloEXC ValueError could not convert string to float: 'k0఑--------k1఑--------k2'
# signature: CPython: ['#', '#']helloEXC ValueError could not convert string to float: 'k#\u#c#--------k#\u#c#--------k#' | mm: ['#', '#']helloEXC ValueError could not convert string to float: 'k#఑--------k#఑--------k#'
import math
try:
    v6 = chr(3089).expandtabs().ljust(9, chr(45)).join(list({'k0': 42, 'k1': 3, 'k2': 42}))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print(sorted([str(q) for q in sorted({0, 0, 1000})], key=len), end='hello')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    v7 = float(v6)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
