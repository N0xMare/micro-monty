# difffuzz divergence, seed 15288
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u1af4': -152}
#   mm:      {'᫴': -152}
# signature: CPython: {'\u#af#': -#} | mm: {'᫴': -#}
import math
try:
    def f10(p0, p1, p2, *rest, kw=3):
        data = ([len(w) for w in chr(713).splitlines()] + list(range(4, 9, -1)))
        print({k: v for k, v in zip(list(chr(6900).partition(',')[0]), list(map(lambda q: q * -3 + 1, ((p1[::1] * 1) + [(3 ** 50 + i11) for i11 in range(-1, 4)])))) if v})
        try:
            print(chr(round((round(math.log2(3.0)) / 0))))
        except LookupError as e:
            print("caught", type(e).__name__, e, e.args)
    print("f10(((([5] + list(reversed([0, 2]))) * 3) * 1), ([a * b for a, b in zip(list(map(lambda q: q * 1 + 1, [])), [len(w) for w in list({'k0': 1})])] + [a * b for a, b in zip(list(reversed(([] + [42, 0, 10, 3, -17]))), list(range(-3, -1, 2)))]), f'{divmod(-1, 1000)[1]:n}|'.join('-42'.split(' ')[0].join([str(q) for q in [0, 2, 10]]).split(' ')))", f10(((([5] + list(reversed([0, 2]))) * 3) * 1), ([a * b for a, b in zip(list(map(lambda q: q * 1 + 1, [])), [len(w) for w in list({'k0': 1})])] + [a * b for a, b in zip(list(reversed(([] + [42, 0, 10, 3, -17]))), list(range(-3, -1, 2)))]), f'{divmod(-1, 1000)[1]:n}|'.join('-42'.split(' ')[0].join([str(q) for q in [0, 2, 10]]).split(' '))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
