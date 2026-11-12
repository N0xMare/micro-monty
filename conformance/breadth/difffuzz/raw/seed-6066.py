# difffuzz divergence, seed 6066
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: Err10 'H\u1717e\u1717l\u1717l\u1717o\u1717 \u1717W\u1717o\u1717r\u1717l\u1717d' 7 wrapped
#   mm:      Err10 'H᜗e᜗l᜗l᜗o᜗ ᜗W᜗o᜗r᜗l᜗d' 7 wrapped
# signature: CPython: Err# 'H\u#e\u#l\u#l\u#o\u# \u#W\u#o\u#r\u#l\u#d' # wrapped | mm: Err# 'H᜗e᜗l᜗l᜗o᜗ ᜗W᜗o᜗r᜗l᜗d' # wrapped
import math
try:
    class Err10(KeyError):
        def __init__(self, msg, code=0):
            super().__init__(msg)
            self.code = code
    try:
        try:
            raise Err10(chr(5911).join(list('Hello World')), 7)
        except Err10 as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        print(type(outer.__cause__).__name__, outer.__cause__, outer.__cause__.code, outer)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
