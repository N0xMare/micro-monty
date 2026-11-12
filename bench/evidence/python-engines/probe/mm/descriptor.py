class D:
    def __get__(s, o, t): return 42
class C:
    x = D()
r = C().x
print(r)
