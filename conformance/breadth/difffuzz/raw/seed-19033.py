# difffuzz divergence, seed 19033
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0a65\u0a65'
#   mm:      EXC ValueError could not convert string to float: '੥੥'
# signature: CPython: EXC ValueError could not convert string to float: '\u#a#\u#a#' | mm: EXC ValueError could not convert string to float: '੥੥'
import math
try:
    def f18(p0):
        data = [ord(c) for c in '%e' % (float((p0 + p0)), p0,)]
        try:
            print(((i7, sum(list(range(2, 9, 2))), -sum(lst0), ) + tuple(sorted({k19: len(k19) for k19 in p0.split(',')}.items())))[(math.ceil(math.fabs(abs(1.5))) ^ pow((int('caf\u00e9') | sum([])), 11, 7))])
        except ArithmeticError as e:
            print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    lst0[2] = f18(chr(2661))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
