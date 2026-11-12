# with statements: builtin and user context managers, exceptions, suppression.
class CM:
    def __init__(self, name, suppress=False):
        self.name = name
        self.suppress = suppress

    def __enter__(self):
        print('enter', self.name)
        return self.name.upper()

    def __exit__(self, typ, val, tb):
        print('exit', self.name, typ.__name__ if typ else None, val)
        return self.suppress


with CM('a') as x:
    print('body', x)

with CM('b', suppress=True):
    raise ValueError('swallowed')
print('after b')

try:
    with CM('c'):
        raise KeyError('k')
except KeyError as e:
    print('caught', repr(e))

with CM('outer') as o, CM('inner', suppress=True) as i:
    print('both', o, i)
    1 / 0
print('after nested')


def early():
    for n in range(3):
        with CM(f'loop{n}'):
            if n == 0:
                continue
            if n == 1:
                break
    with CM('ret'):
        return 'returned'


print(early())


class Exiting:
    def __enter__(self):
        return self

    def __exit__(self, *args):
        raise RuntimeError('from exit')


try:
    with Exiting():
        raise ValueError('body')
except RuntimeError as e:
    print(repr(e), repr(e.__context__))


class BadEnter:
    def __enter__(self):
        raise OSError('no enter')

    def __exit__(self, *a):
        print('never')


try:
    with BadEnter():
        print('never')
except OSError as e:
    print(e)

try:
    with object():
        pass
except TypeError as e:
    print(e)


class NoExit:
    def __enter__(self):
        return 1


try:
    with NoExit():
        pass
except TypeError as e:
    print(e)

with open if False else CM('expr') as (a, *b):
    print(a, b)


class Gen:
    def __init__(self):
        self.state = []

    def __enter__(self):
        self.state.append('in')
        return self.state

    def __exit__(self, t, v, tb):
        self.state.append('out')
        return isinstance(v, ZeroDivisionError)



# The protocol errors name the type by its qualified name, as CPython's
# %T (difffuzz seed 20152509): a class nested in a function shows
# 'f.<locals>.C', a module-level one its plain name.
def nested():
    class Plain:
        pass

    class Half:
        def __enter__(self):
            return self

    return Plain(), Half()


for obj in nested():
    try:
        with obj:
            pass
    except TypeError as e:
        print(e)


g = Gen()
with g as st:
    st.append('body')
    1 / 0
print(g.state)
with CM('final'):
    raise TypeError('escapes')
