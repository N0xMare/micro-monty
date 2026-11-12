# difffuzz divergence, seed 20959
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err4 '\u1a1d' 1 wrapped
#   mm:      Err4 '᨝' 1 wrapped
# signature: CPython: Err# '\u#a#d' # wrapped | mm: Err# '᨝' # wrapped
import math
try:
    if ((tuple(sorted(dict(sorted({'k0': 5, 'k1': 0, 'k2': -17, 'k3': 2}.items(), key=lambda kv: kv[1])).items())) > ()[0:-6:-2][:-2]) or any([i + v for i, v in enumerate(list(reversed([1, 3, 42, 2, 5])), 1)])):
        class Err4(KeyError):
            def __init__(self, msg, code=0):
                super().__init__(msg)
                self.code = code
        try:
            try:
                raise Err4(chr(6685), 1)
            except Err4 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
        def f5(p0, *rest, kw=3):
            try:
                print({}['{}-{!r}-{:>4}'.format(int('hello'.ljust(3, chr(42)), 2), '{}-{!r}-{:>4}'.format('AbC'.find('\U0001f600 ok'), '-42', 255), abs(pow(0, 26, 2 ** 61 - 1)).bit_length()).partition(',')[0]])
            finally:
                print("finally", math.ceil((+math.log10(1e-05) // 7.125)))
        try:
            try:
                raise Err8('a'.replace(('{}-{!r}-{:>4}'.format((255 if True else 255), 'a'.split(None)[-2], (100 % 1)) + '%-6s|' % (-(10 ** 25), '{}-{!r}-{:>4}'.format(7, 'caf\u00e9', -3),)), ('-42' + f'{None:*^9} and {[100]}|{255:>8} '.rjust(9, chr(45)))), 6)
            except Err8 as inner:
                raise RuntimeError("wrapped") from inner
        except RuntimeError as outer:
            print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
