# exceptions: raise and catch in a hot loop: a builtin exception raised by a
# called function, a custom subclass carrying a field, try/else/finally, and
# a KeyError from a dict lookup.
class Invalid(ValueError):
    def __init__(self, code):
        super().__init__(code)
        self.code = code


def check(i):
    if i % 3 == 0:
        raise ValueError('multiple of three')
    return i


def validate(i):
    if i % 5 == 0:
        raise Invalid(i % 11)
    return 1


caught = 0
codes = 0
passed = 0
cleanups = 0
missing = 0
table = {i: i for i in range(0, 1000, 2)}
for i in range(100000):
    try:
        passed += check(i) % 2
    except ValueError:
        caught += 1
    try:
        validate(i)
    except Invalid as error:
        codes += error.code
    else:
        passed += 1
    finally:
        cleanups += 1
    try:
        passed += table[i % 1000]
    except KeyError:
        missing += 1
print(caught, codes, passed, cleanups, missing)
