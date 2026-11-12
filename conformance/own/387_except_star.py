# except* as CPython 3.14 runs it: splitting a group (nested ones too) among
# the clauses, a naked exception wrapped in a group, the rest re-raised,
# what the clauses raise merged with it (_PyExc_PrepReraiseStar), the
# TypeErrors for ExceptionGroup/non-class handlers, else/finally, and the
# tracebacks of the re-raised and the new groups; a falsy group subclass.
import sys, traceback
def show(label, f):
    try:
        f()
        print(label, 'no exception')
    except BaseException as e:
        print(label, type(e).__name__, repr(e))

def t1():
    try:
        raise ExceptionGroup('eg', [ValueError(1), TypeError(2), ValueError(3)])
    except* ValueError as e:
        print('V', repr(e), repr(sys.exception()), e is sys.exception())
    except* TypeError as e:
        print('T', repr(e))
show('t1', t1)

def t2():
    try:
        raise ExceptionGroup('eg', [ValueError(1), KeyError(2), OSError(3)])
    except* ValueError:
        print('V only')
show('t2', t2)

def t3():
    try:
        raise ValueError('naked')
    except* ValueError as e:
        print('naked', repr(e), repr(e.exceptions[0]), e.exceptions[0].__traceback__ is not None)
show('t3', t3)

def t4():
    try:
        raise KeyError('naked')
    except* ValueError as e:
        print('no')
show('t4', t4)

def t5():
    try:
        raise ExceptionGroup('eg', [ValueError(1), TypeError(2)])
    except* ValueError as e:
        raise RuntimeError('new') 
    except* TypeError:
        raise
show('t5', t5)

def t6():
    try:
        raise ExceptionGroup('eg', [ValueError(1)])
    except* ValueError as e:
        raise
show('t6', t6)

def t7():
    try:
        raise ExceptionGroup('eg', [ValueError(1)])
    except* ValueError as e:
        raise e
show('t7', t7)

def t8():
    try:
        raise ValueError('n')
    except* ValueError as e:
        raise
show('t8', t8)

def t9():
    try:
        raise ValueError('n')
    except* ValueError as e:
        raise TypeError('x')
show('t9', t9)

def t10():
    try:
        try:
            raise ExceptionGroup('eg', [ValueError(1)])
        except* ExceptionGroup:
            pass
    except TypeError as e:
        print('TE', e, repr(e.__context__))
    try:
        try:
            raise ValueError
        except* (KeyError, BaseExceptionGroup):
            pass
    except TypeError as e:
        print('TE', e)
    try:
        try:
            raise ValueError
        except* 42:
            pass
    except TypeError as e:
        print('TE', e)
show('t10', t10)

def t11():
    for x in [1]:
        try:
            raise ExceptionGroup('eg', [ValueError(1), TypeError(2)])
        except* ValueError:
            print('v')
        except* TypeError:
            print('t')
        else:
            print('else')
        finally:
            print('finally')
    try:
        pass
    except* ValueError:
        pass
    else:
        print('else ran')
    finally:
        print('fin2')
show('t11', t11)

def t12():
    try:
        raise ExceptionGroup('eg', [ValueError(1), ExceptionGroup('in', [TypeError(2), ValueError(3)]), OSError(4)])
    except* ValueError as e:
        print('V', repr(e))
    except* (TypeError, OSError) as e:
        print('TO', repr(e))
    print('e' in dir())
show('t12', t12)

def t13():
    try:
        raise BaseExceptionGroup('b', [KeyboardInterrupt(), ValueError()])
    except* KeyboardInterrupt as e:
        print('KI', repr(e))
show('t13', t13)

def t14():
    eg = ExceptionGroup('eg', [ValueError(1), TypeError(2)])
    try:
        raise eg
    except* ValueError:
        raise KeyError('k')
show('t14', t14)

def t15():
    try:
        raise ExceptionGroup('eg', [ValueError(1), TypeError(2)])
    except* Exception as e:
        print('all', repr(e), e is not None)
show('t15', t15)

def t16():
    try:
        raise ExceptionGroup('eg', [ValueError(1), TypeError(2)])
    except* ValueError:
        try:
            raise OSError('inner')
        except OSError:
            pass
        print(repr(sys.exception()))
    except* TypeError:
        print('second', repr(sys.exception()))
    print(repr(sys.exception()))
show('t16', t16)

def t17():
    try:
        raise ExceptionGroup('eg', [ValueError(1), TypeError(2)])
    except* ValueError:
        raise OSError('a')
    except* TypeError:
        raise OSError('b')
try:
    t17()
except ExceptionGroup as e:
    print(repr(e), [repr(x.__context__) for x in e.exceptions], e.__context__, e.__cause__)
def t18():
    try:
        raise ExceptionGroup('eg', [ValueError(1)])
    except* ValueError as e:
        e.add_note('note')
        raise
try:
    t18()
except ExceptionGroup as e:
    print(repr(e), getattr(e, '__notes__', None))
# A falsy group still matches (the test is `is None`).
class FalsyEG(ExceptionGroup):
    def __bool__(self):
        return False
    def derive(self, excs):
        return FalsyEG(self.message, excs)
try:
    try:
        raise FalsyEG("eg", [TypeError(1), ValueError(2)])
    except *TypeError as e:
        tes = e
        raise
    except *ValueError as e:
        ves = e
        pass
except Exception as e:
    exc = e
for e in [tes, ves, exc]:
    print(bool(e), type(e).__name__, repr(e))
import traceback
def f(n):
    try:
        if n == 0:
            raise ExceptionGroup('eg', [ValueError(1), TypeError(2)])
        raise ValueError('naked')
    except* ValueError as e:
        if n == 0:
            raise RuntimeError('new')
        raise
    except* TypeError:
        raise
for n in (0, 1):
    try:
        f(n)
    except BaseException as e:
        traceback.print_exc()
def g():
    try:
        raise ValueError('x')
    except* ValueError:
        raise KeyError('k')
try:
    g()
except KeyError:
    traceback.print_exc()
f(0)
