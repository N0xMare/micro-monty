class C:
    __slots__ = ("a",)
c = C()
c.a = 1
r = c.a
print(r)
