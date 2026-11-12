# traceback.TracebackException as CPython 3.14 has it: from_exception,
# format, format_exception_only, print, stack, __str__ and exc_type_str,
# chains, groups, notes and SyntaxError locations. "Did you mean" hints
# follow CPython's rules: a NameError's needs a traceback (the raising
# frame), its stdlib note does not; AttributeError and ImportError hints work
# without one; `self.name` is suggested inside a method.
import io
import sys
import traceback
from traceback import TracebackException


def show(lines):
    for line in lines:
        print(repr(line))


def inner(n):
    return 10 // n


def outer(n):
    return inner(n) + 1


try:
    outer(0)
except ZeroDivisionError as e:
    te = TracebackException.from_exception(e)
    print(type(te).__name__, str(te), te.exc_type_str)
    print(len(te.stack), [f.name for f in te.stack], [f.lineno for f in te.stack])
    print([f.line for f in te.stack])
    show(te.format())
    show(te.format_exception_only())
    print(te.__cause__, te.__context__, te.exceptions, te.__suppress_context__)
    show(TracebackException.from_exception(e, limit=1).format())
    show(TracebackException.from_exception(e, limit=-1).format())
    show(TracebackException(type(e), e, None).format())
    print(TracebackException.from_exception(e) == te)

# print() goes to stderr unless given a file.
try:
    {}['k']
except KeyError as e:
    buf = io.StringIO()
    TracebackException.from_exception(e).print(file=buf)
    print(buf.getvalue(), end='')
    TracebackException.from_exception(e).print(file=sys.stdout, chain=False)


# Chains: cause, context, suppressed context, and compact.
def chained():
    try:
        int('x')
    except ValueError as e:
        raise RuntimeError('wrapped') from e


try:
    chained()
except RuntimeError as e:
    te = TracebackException.from_exception(e)
    print(type(te.__cause__).__name__, str(te.__cause__), te.__context__ is None)
    show(te.format())
    show(te.format(chain=False))

try:
    try:
        [][1]
    except IndexError:
        raise KeyError('during')
except KeyError as e:
    te = TracebackException.from_exception(e)
    print(te.__cause__, str(te.__context__))
    show(te.format())
    print(TracebackException.from_exception(e, compact=True).__context__ is not None)

try:
    try:
        [][1]
    except IndexError:
        raise KeyError('quiet') from None
except KeyError as e:
    te = TracebackException.from_exception(e)
    print(te.__suppress_context__, te.__context__ is not None)
    print(TracebackException.from_exception(e, compact=True).__context__)
    show(te.format())

# Exception groups: exceptions, show_group, max_group_width/depth.
try:
    raise ExceptionGroup('eg', [ValueError(1), TypeError('two'),
                                ExceptionGroup('in', [KeyError(3)])])
except ExceptionGroup as e:
    te = TracebackException.from_exception(e)
    print([str(x) for x in te.exceptions])
    show(te.format())
    show(te.format_exception_only(show_group=True))
    show(TracebackException.from_exception(e, max_group_width=1).format())
    show(TracebackException.from_exception(e, max_group_depth=1).format())

# Notes, and a custom __str__.
e = ValueError('noted')
e.add_note('first')
e.add_note('two\nlines')
show(TracebackException.from_exception(e).format_exception_only())
e.__notes__ = 'not a list'
show(TracebackException.from_exception(e).format_exception_only())


class Loud(Exception):
    def __str__(self):
        return 'LOUD'


class Broken(Exception):
    def __str__(self):
        raise RuntimeError


class Empty(Exception):
    pass


for e in (Loud('x'), Broken('y'), Empty(), Empty('')):
    te = TracebackException.from_exception(e)
    print(str(te), te.exc_type_str)
    show(te.format_exception_only())
show(traceback.format_exception_only(None))
show(TracebackException(None, None, None).format_exception_only())


# "Did you mean": NameError needs the traceback.
def name_error():
    total = 1
    return totl


try:
    name_error()
except NameError as e:
    print(str(TracebackException.from_exception(e)))
    print(str(TracebackException(type(e), e, None)))
    show(traceback.format_exception_only(e))
    show(traceback.format_exception(e)[-1:])

try:
    math.floor(1)
except NameError as e:
    print(str(TracebackException(type(e), e, None)))
    print(str(TracebackException.from_exception(e)))
print(str(TracebackException.from_exception(NameError('x', name='json'))))
print(str(TracebackException.from_exception(NameError('x', name='jsn'))))


class Box:
    def __init__(self):
        self.value = 1

    def get(self):
        return value

    def missing(self):
        return nothing_here


for m in (Box().get, Box().missing):
    try:
        m()
    except NameError as e:
        print(str(TracebackException.from_exception(e)))

# AttributeError and ImportError hints work without a traceback.
try:
    [].apend(1)
except AttributeError as e:
    print(str(TracebackException(type(e), e, None)))
    print(str(TracebackException.from_exception(e)))
print(str(TracebackException.from_exception(AttributeError('x', name='uper', obj='s'))))
try:
    from math import sinn
except ImportError as e:
    print(str(TracebackException(type(e), e, None)))
    show(traceback.format_exception_only(e))
try:
    import json
    json.dumbs
except AttributeError as e:
    print(str(TracebackException(type(e), e, None)))


class Point:
    def __init__(self):
        self.x = 1
        self.y = 2
        self.xy = 3


try:
    Point().yx
except AttributeError as e:
    print(str(TracebackException.from_exception(e)))

# SyntaxError: the location lines, then the message.
for args in [('bad', ('<unknown>', 1, 3, '1 +\n', 1, 4)),
             ('bad', ('f.py', 2, 5, '    x = = 1\n', 2, 8)),
             ('bad', ('f.py', 2, None, '  x\n', 2, None)),
             ('bad', (None, 3, 1, 'abc', 3, 2)),
             ('bad', ('dir/f.py', None, None, None, None, None)),
             ('no tuple',),
             ('', ('f.py', 1, 1, 'x', 1, 1)),
             ('wide', ('f.py', 1, 20, 'ab\n', 1, 30)),
             ('multi', ('f.py', 1, 2, 'abcdef\n', 2, 1))]:
    e = SyntaxError(*args)
    print(repr(str(e)), e.msg, e.filename, e.lineno, e.offset, repr(e.text),
          e.end_lineno, e.end_offset, e.print_file_and_line)
    te = TracebackException.from_exception(e)
    print(te.filename, repr(te.lineno), repr(te.end_lineno), te.offset, te.msg)
    show(te.format_exception_only())
e = IndentationError('unexpected indent', ('f.py', 4, 3, '  y\n', 4, 4))
show(traceback.format_exception_only(e))
e.lineno = 7
e.msg = 'changed'
print(str(e))
show(traceback.format_exception_only(e))

# The module-level functions are TracebackException underneath.
try:
    outer(0)
except ZeroDivisionError as e:
    print(traceback.format_exception(e) == list(TracebackException.from_exception(e).format()))
    print(''.join(traceback.format_exception(e, limit=1)), end='')
    show(traceback.format_exception_only(type(e), e))
    buf = io.StringIO()
    traceback.print_exception(e, file=buf)
    print(buf.getvalue() == ''.join(traceback.format_exception(e)))
