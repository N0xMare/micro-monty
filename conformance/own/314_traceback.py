# traceback: the formatting functions match the engine's own report of an
# uncaught exception, which matches CPython 3.14's (docs/spec.md §4, §12).
import traceback, sys
def inner(x):
    return 1 / x
def outer():
    return inner(0)
try:
    outer()
except ZeroDivisionError as e:
    print(traceback.format_exc(), end='')
    traceback.print_exc()
    print(traceback.format_exception(e))
    print(traceback.format_exception(type(e), e, e.__traceback__))
    print(traceback.format_exception(type(e), e, None))
    print(traceback.format_exception_only(e))
    print(traceback.format_tb(e.__traceback__))
    print(traceback.format_tb(e.__traceback__, limit=1))
    print(traceback.format_tb(e.__traceback__, limit=-1))
    print(traceback.extract_tb(e.__traceback__))
    print([tuple(f) for f in traceback.extract_tb(e.__traceback__)])
    tb = e.__traceback__
    print(type(tb).__name__, tb.tb_lineno, tb.tb_next.tb_lineno, tb.tb_next.tb_next.tb_lineno, tb.tb_next.tb_next.tb_next)
    t, v, tb2 = sys.exc_info()
    print(t, v, type(tb2).__name__)
    traceback.print_exception(*sys.exc_info())
    traceback.print_tb(tb, file=sys.stdout)
print(sys.exc_info(), traceback.format_exc())
try:
    try:
        {}['k']
    except KeyError as k:
        raise ValueError('bad') from k
except ValueError as e:
    print(''.join(traceback.format_exception(e)))
    print(''.join(traceback.format_exception(e, chain=False)))
try:
    try:
        nme
    except NameError:
        int('x')
except ValueError as e:
    e.add_note('a note')
    traceback.print_exc(file=sys.stdout)
print(traceback.format_exception_only(None), traceback.format_exception(None))
try:
    traceback.format_exception(1)
except TypeError as e:
    print(e)
try:
    traceback.format_exception(ValueError, ValueError())
except ValueError as e:
    print(e)
def rec(n):
    return rec(n + 1) if n < 10 else 1/0
try:
    rec(0)
except ZeroDivisionError:
    traceback.print_exc(file=sys.stdout)
    traceback.print_exc(limit=2, file=sys.stdout)
e = ValueError('x')
print(traceback.format_exception(e))
try:
    raise ValueError('wt').with_traceback(None)
except ValueError as e:
    print(traceback.format_exc())
from contextlib import contextmanager, ExitStack


@contextmanager
def cm():
    try:
        yield
    finally:
        pass


def body():
    with cm():
        1 / 0


for wrap in (False, True):
    try:
        if wrap:
            with ExitStack() as s:
                s.enter_context(cm())
                body()
        else:
            body()
    except ZeroDivisionError:
        traceback.print_exc(file=sys.stdout)


class Seen:
    def __enter__(self):
        return self

    def __exit__(self, t, v, tb):
        print(t.__name__, type(tb).__name__, tb.tb_lineno, tb.tb_next)
        print(''.join(traceback.format_tb(tb)), end='')
        return True


with Seen():
    raise KeyError('seen')
e = KeyError('cleared')
try:
    raise e
except KeyError:
    e.__traceback__ = None
    print(e.__traceback__, traceback.format_exception(e))
try:
    e.__traceback__ = 5
except TypeError as x:
    print(x)
print(traceback.format_list([('f.py', 3, 'fn', 'x = 1'), ('g.py', 4, 'gn', '')]))
traceback.print_list([('f.py', 3, 'fn', 'x = 1')], file=sys.stdout)
fs = traceback.FrameSummary('f.py', 3, 'fn', line='  y = 2  ')
print(fs, fs.line, list(fs), fs == ('f.py', 3, 'fn', 'y = 2'), len(fs), fs[2])
try:
    traceback.print_last()
except ValueError as x:
    print(x)
inner(0)
