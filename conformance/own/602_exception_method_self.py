# BaseException.with_traceback and add_note through the type, on a `self`
# that is not an exception: CPython's descriptor refuses it. Without the
# check they wrote the object's slots: a function lost its closure, a class
# its namespace, and any object got a `__notes__` list.
import sys


def outer():
    x = 41

    def inner():
        return x + 1
    return inner


class C:
    attr = 'kept'

    def m(self):
        return 'm'


try:
    1 / 0
except ZeroDivisionError as e:
    tb = e.__traceback__

f = outer()
c = C()
selves = [f, C, c, C.m, c.m, len, [].append, int, object, 1, 'a', None, (), [], {}, sys.version_info,
          sys, tb, (i for i in ()), type, NotImplemented, 1.5, b'', range(1)]
for name in ('with_traceback', 'add_note'):
    for owner in (BaseException, KeyError):
        m = getattr(owner, name)
        for s in selves:
            for arg in (None, tb, 'n'):
                try:
                    m(s, arg)
                    print('accepted', name, owner.__name__, type(s).__name__)
                except TypeError as e:
                    print(e)
                    break
print(f(), C.attr, C().m(), c.m(), sorted(k for k in vars(C) if not k.startswith('__')), vars(c))
print([hasattr(s, '__notes__') for s in selves], len('ab'), int('7'), list(sys.version_info)[:2])

# On an exception, of the owner's class or not, they work as bound.
e = ValueError('v')
print(BaseException.add_note(e, 'a'), KeyError.add_note(e, 'b'), e.__notes__)
print(BaseException.with_traceback(e, tb) is e, e.__traceback__ is not None, KeyError.with_traceback(e, None) is e,
      e.__traceback__)
for bad in (1, 'tb', e):
    try:
        BaseException.with_traceback(e, bad)
    except TypeError as err:
        print(err)
try:
    BaseException.add_note(e, 1)
except TypeError as err:
    print(err)


class Mine(Exception):
    pass


g = ExceptionGroup('g', [Mine('m')])
for x in (Mine('m'), g, MemoryError(), StopIteration(3)):
    BaseException.add_note(x, 'n')
    print(type(x).__name__, x.__notes__, BaseException.with_traceback(x, None) is x)
