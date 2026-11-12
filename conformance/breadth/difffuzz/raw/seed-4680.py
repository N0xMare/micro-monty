# difffuzz divergence, seed 4680
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'\u1a1c': 4}AbCErr14 a,b,,c 5 wrapped
#   mm:      {'᨜': 4}AbCErr14 a,b,,c 5 wrapped
# signature: CPython: {'\u#a#c': #}AbCErr# a,b,,c # wrapped | mm: {'᨜': #}AbCErr# a,b,,c # wrapped
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print(dict(zip(sorted(chr(6684).splitlines(), key=str.lower), [i + v for i, v in enumerate((list(range(0, -5, 2)) + list(filter(lambda q: q > -1, lst0))), 1)])), end='AbC')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    class Err14(Exception):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err14('a,b,,c', 5)
        except Err14 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
