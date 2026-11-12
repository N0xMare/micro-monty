# difffuzz divergence, seed 36628
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['7', '0', '-', "'", '0', '0', '0', '0', '0', '0', '\\', 'u', '2', 'e', '5', 'e', "'", '-', ' ', ' ', ' ', '0', '1']
#   mm:      ['7', '0', '-', "'", '0', '0', '0', '0', '0', '0', '⹞', "'", '-', ' ', ' ', ' ', '0', '1']
# signature: CPython: ['#', '#', '-', "'", '#', '#', '#', '#', '#', '#', '\\', 'u', '#', 'e', '#', 'e', "'", '-', ' ', ' ' | mm: ['#', '#', '-', "'", '#', '#', '#', '#', '#', '#', '⹞', "'", '-', ' ', ' ', ' ', '#', '#']
import math
try:
    n19 = 1
    while n19 > 0:
        n19 -= 1
        data = [len(w) for w in [str(q) for q in [len(w) for w in list({})]]]
        n20 = 5
        while n20 > 0:
            n20 -= 1
            def f21(p0, p1):
                print(list(p1.swapcase().zfill(8).lower()))
            print("f21((int(abs(2.5)) << (2 ** 64 % 70)), '{}-{!r}-{:>4}'.format(n19, chr(11870).zfill(7), int(float(0))).join(list({str(q): q for q in [len(w) for w in ['  pad  ', 'a', 'a']]})))", f21((int(abs(2.5)) << (2 ** 64 % 70)), '{}-{!r}-{:>4}'.format(n19, chr(11870).zfill(7), int(float(0))).join(list({str(q): q for q in [len(w) for w in ['  pad  ', 'a', 'a']]}))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
