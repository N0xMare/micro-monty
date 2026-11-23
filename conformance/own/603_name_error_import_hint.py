# A NameError for a module name ends with CPython's "Did you forget to
# import 'x'?" when the module imports here, after a "Did you mean" when one
# applies; a name that is not a stdlib module gets no hint. Names CPython
# hints that micro-monty cannot import are not here: they get no hint, or the
# sandbox's redirect sentence (docs/user-guide.md, "Did you forget to
# import").
import traceback


def show(lookup):
    try:
        lookup()
    except NameError as e:
        print(traceback.format_exception(e)[-1], end='')


# One function per name, each raising NameError for it.
LOOKUPS = {
    'json': lambda: json,
    're': lambda: re,
    'math': lambda: math,
    'sys': lambda: sys,
    'collections': lambda: collections,
    'itertools': lambda: itertools,
    'functools': lambda: functools,
    'datetime': lambda: datetime,
    'dataclasses': lambda: dataclasses,
    'typing': lambda: typing,
    'textwrap': lambda: textwrap,
    'tomllib': lambda: tomllib,
    'fnmatch': lambda: fnmatch,
    'binascii': lambda: binascii,
    'numbers': lambda: numbers,
    'numpy': lambda: numpy,
    'jsn': lambda: jsn,
    'not_a_module': lambda: not_a_module,
}

for lookup in LOOKUPS.values():
    show(lookup)

# With a near name in scope, both sentences.
strings = 1
try:
    string
except NameError as e:
    print(traceback.format_exception(e)[-1], end='')


def inside():
    jsons = 2
    return json


try:
    inside()
except NameError as e:
    print(traceback.format_exception(e)[-1], end='')

# An imported module deleted again is a name like any other.
import heapq

del heapq
try:
    heapq
except NameError as e:
    print(traceback.format_exception(e)[-1], end='')
