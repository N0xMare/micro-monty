# repr(super(...)) names both types by their bare names, builtin types
# included; bound to a class, the object type is that class.
class A:
    def f(self):
        return repr(super())
class B(A):
    def f(self):
        return repr(super())
def outer():
    class C(B):
        def f(self):
            return repr(super())
    return C
print(repr(super(int, 1)), repr(super(B, B())), B().f(), outer()().f(), repr(super(A, B)), repr(super(object, 'x')))
print(repr(super(type, int)), repr(super(int, bool)))
