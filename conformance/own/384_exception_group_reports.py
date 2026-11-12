# Exception groups in reports as CPython 3.14 draws them: nested groups,
# chained causes inside a group, the 15-wide cut-off, through
# traceback.format_exception/print_exc and uncaught; and
# format_exception_only calling a class's own __str__.
import traceback
class E(Exception):
    def __str__(self): return "custom"
print(traceback.format_exception_only(E()))
def f():
    try:
        raise KeyError('ctx')
    except KeyError:
        raise ExceptionGroup('outer', [ValueError(1), ExceptionGroup('inner', [TypeError('t'), OSError('o')]), RuntimeError('r')]) from ZeroDivisionError('zd')
try:
    f()
except ExceptionGroup as e:
    eg = e
big = ExceptionGroup('big', [ValueError(i) for i in range(17)])
try:
    raise ExceptionGroup('wrap', [eg, big])
except ExceptionGroup as e:
    print(''.join(traceback.format_exception(e)), end='')
    traceback.print_exc()
def g():
    raise ExceptionGroup('wrap', [eg, big])
g()
