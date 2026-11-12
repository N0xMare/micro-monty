# difffuzz divergence, seed 35037
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ('\u13ff',)  pad  EXC NameError name 'i36' is not defined
#   mm:      ('᏿',)  pad  EXC NameError name 'i36' is not defined
# signature: mm: ('᏿',)  pad  EXC NameError name 'i#' is not defined
import math
try:
    print((chr(5119), ), sep='AbC', end='  pad  ')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f50(p0, *rest, kw=3):
        while n51 > 0:
            def f53():
                return (''.replace('AbC', 'mississippi') + '{}-{!r}-{:>4}'.format(0, 'Hello World', 5)).partition(' ')[0].rstrip()
                if k % 4 == 0:
                    yield k * -1
    print("f50((3 ** 50 if (not f'{i36:n}|'.isidentifier()) else (sum(list(range(4, 4, -2))) >> (-10 ** 30 + 7 % 9))))", f50((3 ** 50 if (not f'{i36:n}|'.isidentifier()) else (sum(list(range(4, 4, -2))) >> (-10 ** 30 + 7 % 9)))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
