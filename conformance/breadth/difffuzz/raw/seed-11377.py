# difffuzz divergence, seed 11377
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: "1267650600228229401496703205375-'\\u0aa9'-1267650600228229401496703205375"
#   mm:      EXC ValueError could not convert string to float: "1267650600228229401496703205375-'઩'-1267650600228229401496703205375"
# signature: CPython: EXC ValueError could not convert string to float: "#-'\\u#aa#'-#" | mm: EXC ValueError could not convert string to float: "#-'઩'-#"
import math
try:
    def f11(p0, p1, p2):
        def make12(start):
            total = start
            def step(by=1):
                return total
        s = make12((sum([ord(c) for c in repr(1e10)]) << (int(float('{}-{!r}-{:>4}'.format(p0, p1, p0))) % 70)))
        def gen13(n):
            for k in range(n):
                    yield k * -1
    print("f11((1 << 100) - 1, chr(2729), f'{3=}|{None!r} '.upper()[7:0][-3:6:-1])", f11((1 << 100) - 1, chr(2729), f'{3=}|{None!r} '.upper()[7:0][-3:6:-1]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
