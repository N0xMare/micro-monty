# difffuzz divergence, seed 40374
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 3, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u0a43----'
#   mm:      EXC ValueError could not convert string to float: '੃----'
# signature: CPython: EXC ValueError could not convert string to float: '\u#a#----' | mm: EXC ValueError could not convert string to float: '੃----'
import math
try:
    for i1 in range(1, 4):
        n2 = 3
        while n2 > 0:
            n2 -= 1
            if isinstance((-(10 ** 25) + abs((5 >> (n2 % 9))).bit_length()), int):
                v3 = '{}-{!r}-{:>4}'.format(int(1e10), ('-42' * (n2 % 4)), int(1e300)).partition(',')[1].rjust(3, chr(42))
            def make5(start):
                total = start
                def step(by=1):
                    return total
            try:
                try:
                    raise Err6((chr(597).zfill(7) * ([i + v for i, v in enumerate(list(range(5, 10, 3)), 0)][round(round(1.5, -2))] % 4)), 8)
                except Err6 as inner:
                    raise RuntimeError("wrapped") from inner
            except RuntimeError as outer:
                print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f10(p0, p1, p2):
        try:
            print(float(chr(2627)[len({})].ljust(5, chr(45))))
        finally:
            print("finally", math.gcd(abs(sum((([7, 255, 1] * 3) + list(range(-2, 8, -1))))) % 30))
    print("f10([], 5, f10(sorted((list(filter(lambda q: q > 5, [-1, 7])) + [len(w) for w in ['x\\ny']]), key=str), math.ceil(abs(n2) % 30), '{}-{!r}-{:>4}'.format(5, 'mississippi', n2).replace((v3 + '-42'), v3.upper()).lstrip()))", f10([], 5, f10(sorted((list(filter(lambda q: q > 5, [-1, 7])) + [len(w) for w in ['x\ny']]), key=str), math.ceil(abs(n2) % 30), '{}-{!r}-{:>4}'.format(5, 'mississippi', n2).replace((v3 + '-42'), v3.upper()).lstrip())))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
