# difffuzz divergence, seed 39119
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u0b45']ßEXC TypeError unsupported format string passed to NoneType.__format__
#   mm:      ['୅']ßEXC TypeError unsupported format string passed to NoneType.__format__
# signature: CPython: ['\u#b#']ßEXC TypeError unsupported format string passed to NoneType.__format__ | mm: ['୅']ßEXC TypeError unsupported format string passed to NoneType.__format__
import math
try:
    print(chr(2885).split(), end='\u00df')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def make19(start):
        total = start
        def step(by=1):
            return total
    s = make19(len({'AbC'.removeprefix(repr(['caf\u00e9', '\u00df', '  pad  ', 'AbC'])): sum([len(w) for w in []]), str(int(7.125)): len(f'{None:.2} and {[5]} {100} '), 'x\ny'.split(' ')[-1].join('AbC'.splitlines()): n8}))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
