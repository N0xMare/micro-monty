# difffuzz divergence, seed 17011
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError could not convert string to float: '\u0c3a' ("could not convert string to float: '\\u0c3a'",)
#   mm:      caught ValueError could not convert string to float: '఺' ("could not convert string to float: '఺'",)
# signature: CPython: caught ValueError could not convert string to float: '\u#c#a' ("could not convert string to float: ' | mm: caught ValueError could not convert string to float: '఺' ("could not convert string to float: '఺'",)
import math
try:
    for i15 in sorted(set([-1, 1000, 1000][:]))[:]:
        if i15 == 2:
            continue
        n16 = 4
        while n16 > 0:
            n16 -= 1
            v17 = pow((int(-0.0) if (False or True) else int('%05d' % (1e300,), 36)), 8, 1000)
        def risky():
            try:
                print(float(chr(3130)))
            except (TypeError, ValueError) as e:
                print("caught", type(e).__name__, e, e.args)
        print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
