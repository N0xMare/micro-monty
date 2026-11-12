class A: pass
class B(A): pass
class C(A): pass
class D(B, C): pass
r = [k.__name__ for k in D.__mro__]
print(r)
