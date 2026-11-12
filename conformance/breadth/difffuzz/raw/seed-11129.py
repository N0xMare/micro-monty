# difffuzz divergence, seed 11129
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError could not convert string to float: '\u20cc' ("could not convert string to float: '\\u20cc'",)
#   mm:      caught ValueError could not convert string to float: '⃌' ("could not convert string to float: '⃌'",)
# signature: CPython: caught ValueError could not convert string to float: '\u#cc' ("could not convert string to float: '\ | mm: caught ValueError could not convert string to float: '⃌' ("could not convert string to float: '⃌'",)
import math
try:
    for i3 in range(-3, 9):
        def f5(p0):
            return str(2.5).zfill(7).join('\U0001f600 ok'[2::2].splitlines()).partition('s')[1]
            for k in range(n):
                    yield k * -2
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def risky():
        try:
            print(float(chr(8396)[~int((5 / i3))]))
        except ValueError as e:
            print("caught", type(e).__name__, e, e.args)
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
