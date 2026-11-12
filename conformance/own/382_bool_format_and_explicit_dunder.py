# bool formats as int whenever the spec is non-empty (as str when empty),
# and builtin values answer an explicit `__format__` call as `format()`.

print(repr(format(True, "5")), repr(f"{True:>5}"), repr("{:d}".format(True)), repr(format(False, "")), repr(f"{True}"), repr(format(True, "x")), repr(f"{False:<3}|"), repr(format(True, ".2f")), repr("{!s:5}".format(True)), repr("%5s" % True), repr("%d" % True))
print(repr(format(True, "^7")), repr(f"{True!r:>6}"), repr(True.__format__("5")), repr(True.__format__("")))
print(repr((1).__format__("5")), repr(int.__format__(True, "5")))
print(repr(True.__format__("5")))
class I(int): pass
class S(str): pass
print(repr(I(5).__format__("03")), repr(S("ab").__format__(">4")), repr((2.5).__format__(".1f")), repr("x".__format__("")), repr(False.__format__("")))
print(repr(f"{True:5}|{False:05}|{True:+}|{True:,}|{True:b}"))
class O: pass
try: O().__format__("5")
except TypeError as e: print(e)
