# difffuzz divergence, seed 18613
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u20cc': 100, 'z': 1} [('1', 1), 3]
#   mm:      {'⃌': 100, 'z': 1} [('1', 1), 3]
# signature: CPython: {'\u#cc': #, 'z': #} [('#', #), #] | mm: {'⃌': #, 'z': #} [('#', #), #]
import math
try:
    if all(list(reversed((list({'k0': 255, 'k1': 42, 'k2': 2}.values()) * 1)))):
        def f1(p0, p1, p2):
            print({**dict(zip(chr(8396).removeprefix('{}-{!r}-{:>4}'.format(3, p2, p0)).splitlines(), list(filter(lambda q: q > 4, [i + v for i, v in enumerate(sorted([100]), 0)])))), "z": 1}, [*(tuple(sorted({str(q): q for q in [len(w) for w in ['a']]}.items())) + (3, )[-1::-2]), *list(filter(lambda q: q > 5, [len(w) for w in p2.removeprefix('  pad  ').split()]))])
            if (w := divmod(divmod((max(p1) ** (p1[p0] % 7)), math.factorial(abs(abs(p0).bit_length()) % 30))[1], p0)[1]) > -2:
                print("walrus", w)
        print("f1(sum([(divmod({'k0': 3, 'k1': 1, 'k2': 255, 'k3': 3}['k1'], (7 * 1))[0] + i7) for i7 in [len(w) for w in list({})] if i7 % 3]), list({k8: len(k8) for k8 in ['']}.values()), chr(11195).replace(str((-17 == 3)).title(), str(1e300).center(7, chr(45))))", f1(sum([(divmod({'k0': 3, 'k1': 1, 'k2': 255, 'k3': 3}['k1'], (7 * 1))[0] + i7) for i7 in [len(w) for w in list({})] if i7 % 3]), list({k8: len(k8) for k8 in ['']}.values()), chr(11195).replace(str((-17 == 3)).title(), str(1e300).center(7, chr(45)))))
        for i9 in range(0, 9):
            def rec10(n):
                if n <= 1:
                    return n
        def risky():
            try:
                print([i + v for i, v in enumerate([100, 7, -17], 0)][int('mississippi'.replace('mississippi'.center(5, chr(45)), 'x\ny'[i9]).swapcase())])
            except ArithmeticError as e:
                print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
