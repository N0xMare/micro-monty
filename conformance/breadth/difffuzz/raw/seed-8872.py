# difffuzz divergence, seed 8872
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u206fabc'
#   mm:      EXC ValueError could not convert string to float: '⁯abc'
# signature: CPython: EXC ValueError could not convert string to float: '\u#fabc' | mm: EXC ValueError could not convert string to float: '⁯abc'
import math
try:
    print(float((chr(8303).removesuffix('123'.partition(',')[1]) + 'AbC'.replace('123', 'a').lower())))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
