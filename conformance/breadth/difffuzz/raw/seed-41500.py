# difffuzz divergence, seed 41500
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError invalid literal for int() with base 10: '\u0e60' ("invalid literal for int() with base 10: '\\u0e60'",)
#   mm:      caught ValueError invalid literal for int() with base 10: '๠' ("invalid literal for int() with base 10: '๠'",)
# signature: CPython: caught ValueError invalid literal for int() with base #: '\u#e#' ("invalid literal for int() with ba | mm: caught ValueError invalid literal for int() with base #: '๠' ("invalid literal for int() with base #
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    n23 = 4
    while n23 > 0:
        data = [ord(c) for c in chr(5142).split(' ')[-1].swapcase()]
        def risky():
            try:
                print(lst0 < '{}-{!r}-{:>4}'.format(int(chr(3680)), 'caf\u00e9'.join(sorted(['hello', 'a,b,,c', 'x\ny', 'tab\there'], key=lambda w: (-len(w), w))), n3).partition(' ')[2])
            except (TypeError, ValueError) as e:
                print("caught", type(e).__name__, e, e.args)
        print(risky())
        while n25 > 0:
            if 'x\ny'.split(',')[-2].split(None)[-1].isdigit():
                print((5 ** (len('%e' % (1e10, divmod(n23, n3)[0],)) % 7)), sep='  pad  ')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
