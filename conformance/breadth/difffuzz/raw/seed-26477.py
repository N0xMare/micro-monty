# difffuzz divergence, seed 26477
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC KeyError '\u2fd6'
#   mm:      EXC KeyError '⿖'
# signature: CPython: EXC KeyError '\u#fd#' | mm: EXC KeyError '⿖'
import math
try:
    def risky():
        try:
            print({k20: len(k20) for k20 in 'a'[:2].swapcase().splitlines()}[chr(12246)])
        except IndexError as e:
            print("caught", type(e).__name__, e, e.args)
    print(risky())
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
