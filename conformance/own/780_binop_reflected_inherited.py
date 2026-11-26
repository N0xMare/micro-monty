# A binary operator whose right operand's class is a subclass of the
# left's: the subclass's reflected method runs first only when the subclass
# overrides it (CPython's `method_is_overloaded`), not when it inherits it
# or holds the base's own function. In-place operators fall back the same
# way, and the left operand's declining method still hands over.


class A:
    def __init__(self, tag):
        self.tag = tag

    def __add__(self, o):
        return f'{self.tag}.add({o.tag})'

    def __radd__(self, o):
        return f'{self.tag}.radd({o.tag})'

    def __mul__(self, o):
        return NotImplemented

    def __rmul__(self, o):
        return f'{self.tag}.rmul({o.tag})'

    def __sub__(self, o):
        return f'{self.tag}.sub({o.tag})'

    __rsub__ = __add__


class B(A):
    pass


class C(A):
    def __radd__(self, o):
        return f'{self.tag}.C.radd({o.tag})'


class D(A):
    __radd__ = A.__radd__


a, b, c, d = A('a'), B('b'), C('c'), D('d')
assert a + b == 'a.add(b)'
assert a + c == 'c.C.radd(a)'
assert a + d == 'a.add(d)'
assert a * b == 'b.rmul(a)'
assert a - b == 'a.sub(b)'
x = a
x += b
assert x == 'a.add(b)'


class I(int):
    def __radd__(self, o):
        return 'I.radd'


assert 1 + I(2) == 'I.radd'
