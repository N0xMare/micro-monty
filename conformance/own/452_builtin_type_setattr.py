# Setting or deleting an attribute of a builtin type is a TypeError; an
# instance of a builtin type raises AttributeError.
def t(f):
    try:
        f()
    except Exception as e:
        print(type(e).__name__, e)


def a():
    int.x = 2


def b():
    del int.x


def c():
    int.real = 1


def d():
    (1).real = 2


def e():
    list.append = 1


def g():
    object.x = 1


def h():
    setattr(str, 'y', 1)


def i():
    delattr(dict, 'keys')


for fn in (a, b, c, d, e, g, h, i):
    t(fn)
