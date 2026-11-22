# The builtins `exit` and `quit` raise SystemExit as `sys.exit` does.
import sys

# `input()`: there is no stdin (CPython's is at its end here, and site's
# `exit` closes it, so this comes first): the prompt is written and the
# fallback is taken.
try:
    name = input('name? ')
except EOFError:
    name = 'fallback'
print(name)


def t(f, *a):
    try:
        f(*a)
        print('returned')
    except SystemExit as e:
        print('SystemExit', repr(e.code), isinstance(e, BaseException), isinstance(e, Exception))


for f in exit, quit, sys.exit:
    t(f)
    t(f, None)
    t(f, 0)
    t(f, 3)
    t(f, 'message')

# Not caught by `except Exception`, and `finally` runs.
try:
    try:
        exit(2)
    except Exception:
        print('wrong handler')
    finally:
        print('finally')
except SystemExit as e:
    print('left with', e.code)


def exit():
    return 'shadowed'


print(exit())
