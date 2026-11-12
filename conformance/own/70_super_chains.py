# super(): zero-argument, two-argument, cooperative __init__, classmethods.
class Root:
    def __init__(self, **kw):
        self.log = ['Root']
        self.rest = kw


class Left(Root):
    def __init__(self, left=0, **kw):
        super().__init__(**kw)
        self.log.append('Left')
        self.left = left


class Right(Root):
    def __init__(self, right=0, **kw):
        super().__init__(**kw)
        self.log.append('Right')
        self.right = right


class Both(Left, Right):
    def __init__(self, **kw):
        super().__init__(**kw)
        self.log.append('Both')


b = Both(left=1, right=2, other=3)
print(b.log, b.left, b.right, b.rest)


class P:
    def f(self):
        return 'P.f'

    @classmethod
    def make(cls):
        return cls.__name__

    @staticmethod
    def st():
        return 'static'


class Q(P):
    def f(self):
        return 'Q.f+' + super(Q, self).f()

    @classmethod
    def make(cls):
        return 'Q:' + super().make()


class R(Q):
    def f(self):
        return 'R.f+' + super(Q, self).f()


print(Q().f(), R().f(), R.make(), Q.make())
print(super(Q, Q()).st(), super(R, R).make())
s = super(Q, Q())
print(type(s).__name__)
try:
    super(Q, 3)
except TypeError as e:
    print(e)


def helper(self):
    return super().f()


class Late(P):
    def f(self):
        return 'Late+' + (lambda: super(Late, self).f())()


print(Late().f())


class Cell:
    def get(self):
        return __class__


print(Cell().get() is Cell)
try:
    super().f()
except RuntimeError as e:
    print(e)
