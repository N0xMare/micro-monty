# Exception constructor keywords as CPython 3.14 takes them: ImportError's
# name, path and name_from (also on ModuleNotFoundError and subclasses, and
# set by the import system), NameError's name and AttributeError's name and
# obj through super().__init__ in user subclasses, and CPython's TypeError for
# a keyword a class does not take.
import traceback


def attrs(e):
    return (e.name, e.path, e.name_from, e.msg)


e = ImportError('x', name='m', path='/p')
print(attrs(e), e.args, str(e), repr(e))
e = ImportError('x', name_from='y')
print(attrs(e))
print(attrs(ImportError()), attrs(ImportError(1, 2)), str(ImportError(1, 2)))
e = ModuleNotFoundError('y', name='q')
print(attrs(e), repr(e), str(e))
e.name = 'changed'
e.path = 3
print(attrs(e))
for cls, kw in [(ImportError, {'bogus': 1}), (ModuleNotFoundError, {'obj': 1}),
                (ImportError, {'name': 'a', 'msg': 'b'}), (ValueError, {'name': 'x'}),
                (NameError, {'obj': 1}), (AttributeError, {'path': 1}),
                (Exception, {'a': 1}), (KeyError, {'name': 'k'})]:
    try:
        cls('m', **kw)
    except TypeError as t:
        print(cls.__name__, t)


# The import system sets them.
def imported(stmt):
    try:
        exec_like[stmt]()
    except ImportError as e:
        print(type(e).__name__, attrs(e))


def from_math():
    from math import sinn


def missing():
    import nosuchmod


def not_package():
    import os.nope


exec_like = {'from': from_math, 'import': missing, 'dotted': not_package}
for k in exec_like:
    imported(k)


# super().__init__ with keywords in user subclasses.
class N(NameError):
    def __init__(self, m, nm):
        super().__init__(m, name=nm)


class A(AttributeError):
    def __init__(self, m, obj):
        super().__init__(m, name='uper', obj=obj)


class A2(AttributeError):
    def __init__(self, m):
        AttributeError.__init__(self, m, name='z', obj=3)


class I(ImportError):
    def __init__(self, m):
        super().__init__(m, name='mod', path='p', name_from='nm')


class M(ModuleNotFoundError):
    pass


n = N('hi', 'foo')
print(n.name, n.args, str(n))
a = A('hey', 'text')
print(a.name, a.obj, a.args)
print(traceback.format_exception_only(a))
print(A2('x').name, A2('x').obj)
i = I('hey')
print(attrs(i), i.args)
m = M('mm', name='pkg')
print(attrs(m), m.args)
e = NameError('x')
e.__init__('y', name='w')
print(e.args, e.name)
e = ImportError('x')
e.__init__('z', path='/q')
print(attrs(e), e.args)


class B(ValueError):
    def __init__(self, m):
        super().__init__(m, name='z')


class C(NameError):
    def __init__(self, m):
        super().__init__(m, bogus='z')


class D(ImportError):
    def __init__(self, m):
        super().__init__(m, obj=1)


for cls in (B, C, D):
    try:
        cls('x')
    except TypeError as t:
        print(cls.__name__, t)

# Raised and reported with the keywords' hints.
try:
    raise N('custom', 'lenn')
except NameError as e:
    print(traceback.format_exception(e)[-1], end='')
try:
    raise A('custom', [])
except AttributeError as e:
    print(traceback.format_exception(e)[-1], end='')
