# difffuzz divergence, seed 19601
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 0: '\u0b80'
#   mm:      EXC ValueError invalid literal for int() with base 0: '஀'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b#' | mm: EXC ValueError invalid literal for int() with base #: '஀'
import math
try:
    n1 = 5
    while n1 > 0:
        for i2 in range(3):
            v3 = {str(q): q for q in (list(reversed([7, 1000, 3, 2])) * 2)[:-4]}
            try:
                print(undefined_name)
            except IndexError as e:
                print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f17(p0, p1, p2):
        while n18 > 0:
            print(list(chr(7974)))
    print("f17(([i + v for i, v in enumerate(list(map(lambda q: q * -2 + 1, [i + v for i, v in enumerate([10, -3], 2)])), 2)] + [(len(v3) + i21) for i21 in range(7)]), int(chr(2944).title(), 0), ('hello' * (n1 % 4)).center(6, chr(45)).strip('%d' % (int('a', 10),))[~pow(abs(-17).bit_length(), 4, -5)])", f17(([i + v for i, v in enumerate(list(map(lambda q: q * -2 + 1, [i + v for i, v in enumerate([10, -3], 2)])), 2)] + [(len(v3) + i21) for i21 in range(7)]), int(chr(2944).title(), 0), ('hello' * (n1 % 4)).center(6, chr(45)).strip('%d' % (int('a', 10),))[~pow(abs(-17).bit_length(), 4, -5)]))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
