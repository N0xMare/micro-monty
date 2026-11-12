# "Did you mean" hints as CPython 3.14's traceback renders them: a
# NameError's needs a traceback (its frame's names), so
# format_exception_only shows none; an AttributeError's comes from its
# `name` and `obj`, with or without one. `name` and `obj` are attributes and
# constructor keywords; the hint follows their current values.
import traceback, math


def only(e):
    return traceback.format_exception_only(e)[-1]


def full(e):
    return traceback.format_exception(e)[-1]


def both(label, e):
    print(label, '|', only(e), end='')
    print(label, '|', full(e), end='')


class C:
    colour = 1


try:
    pirnt
except NameError as e:
    print(e.name, repr(e.args))
    both('global', e)


def f():
    alpha_value = 1
    return alpha_valu


try:
    f()
except NameError as e:
    both('local', e)

try:
    C().color
except AttributeError as e:
    print(e.name, type(e.obj).__name__)
    both('attr', e)

try:
    [].apend
except AttributeError as e:
    print(e.name, e.obj)
    both('builtin', e)

try:
    math.sqr
except AttributeError as e:
    print(e.name, e.obj is math)
    both('module', e)

try:
    from math import sqr
except ImportError as e:
    both('import', e)

e = NameError('x', name='pirnt')
print(e.name, e.args, e.__dict__)
both('hand name', e)
try:
    raise NameError('x', name='pirnt')
except NameError as e:
    both('raised name', e)
try:
    raise NameError('x')
except NameError as e:
    print(e.name)
    both('raised no name', e)
    e.name = 'pirnt'
    both('named later', e)

e = AttributeError('x', name='color', obj=C())
print(e.name, type(e.obj).__name__, e.args)
both('hand attr', e)
both('hand attr no obj', AttributeError('x', name='color'))
e = AttributeError()
print(e.name, e.obj)
try:
    raise AttributeError('y', name='sqr', obj=math)
except AttributeError as e:
    both('raised module', e)

try:
    pirnt
except NameError as e:
    e.name = 'alpha'
    both('renamed', e)
try:
    C().color
except AttributeError as e:
    e.obj = [1]
    e.name = 'apend'
    both('reassigned', e)


class MyName(NameError):
    pass


e = MyName('m', name='pirnt')
print(type(e).__name__, e.name)
try:
    raise e
except NameError as x:
    both('subclass', x)

for bad in (lambda: NameError('x', foo=1), lambda: NameError('x', obj=1),
            lambda: AttributeError('x', name='c', foo=1),
            lambda: KeyError('x', name=1), lambda: MyName('x', foo=1)):
    try:
        bad()
    except TypeError as t:
        print(t)


def g():
    try:
        pirnt
    except NameError as e:
        return e


e = g()
both('returned', e)
try:
    raise ValueError('outer') from e
except ValueError as v:
    print(''.join(traceback.format_exception(v)).count('Did you mean'))
