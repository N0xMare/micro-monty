# difffuzz divergence, seed 21740
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 36: '\u07b7'
#   mm:      EXC ValueError invalid literal for int() with base 36: '޷'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#b#' | mm: EXC ValueError invalid literal for int() with base #: '޷'
import math
try:
    class C8:
        def __init__(self, v, w=0):
            self.w = w
    objs = [C8(((max([ord(c) for c in '3.5']) if 'x\ny'.lstrip().isidentifier() else int(chr(1975), 36)) ^ len(repr({'\u00df': 100, 'caf\u00e9': 2, '-42': 0}))), 1), C8(len(set(([ord(c) for c in 'a'] + list(range(1, -1, -2))))), -4), C8(len('{}-{!r}-{:>4}'.format(int(('\u00df' * (42 % 4))), f'{lst0}{3:+}{lst0}|'.lstrip(), 'a,b,,c'.split('s')[-1].index(repr(())))), 3), C8.make(4)]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
