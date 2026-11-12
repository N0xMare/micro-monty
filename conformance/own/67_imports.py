# import forms, local binding, and import errors for missing names and modules.
import sys
import asyncio as aio
from sys import stdout, stderr as err


def local_import():
    import sys as system
    from asyncio import gather

    return system is sys, gather is aio.gather


print(local_import(), stdout is sys.stdout, err is sys.stderr)
try:
    system
except NameError as e:
    print(e)
for name in ('definitely_not_a_module', 'sys.nope'):
    try:
        if name == 'sys.nope':
            import sys.nope
        else:
            import definitely_not_a_module
    except ModuleNotFoundError as e:
        print(type(e).__name__, e, isinstance(e, ImportError))
try:
    from sys import nothing_here
except ImportError as e:
    print(e)
try:
    sys.nothing_here
except AttributeError as e:
    print(e)
print(sys.stdout.write('w\n'), file=sys.stderr)
print(stdout.flush())
