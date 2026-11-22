# A keyword given twice through `**` names the callee as CPython does: a
# bound builtin method as `list.sort()`, a method of a class and the class
# itself with their module and qualified name.
def t(f):
    try:
        f()
    except TypeError as e:
        print(e)


def f(a):
    pass


class C:
    def __init__(self, a=1):
        pass

    def m(self, a):
        pass

    class In:
        def __init__(self, a=1):
            pass


k = {'key': str}
t(lambda: [1, 2].sort(key=int, **k))
sort = [1, 2].sort
t(lambda: sort(key=int, **k))
t(lambda: list.sort([1], key=int, **k))
t(lambda: sorted([1], key=int, **k))
t(lambda: 'a'.split(sep='x', **{'sep': 'y'}))
t(lambda: 'a'.encode(encoding='x', **{'encoding': 'y'}))
t(lambda: {}.update(a=1, **{'a': 2}))
t(lambda: {}.get(a=1, **{'a': 2}))
t(lambda: ''.join(a=1, **{'a': 2}))
t(lambda: dict(a=1, **{'a': 2}))
t(lambda: f(a=1, **{'a': 2}))
t(lambda: C().m(a=1, **{'a': 2}))
t(lambda: C.m(C(), a=1, **{'a': 2}))
t(lambda: C(a=1, **{'a': 2}))
t(lambda: C.In(a=1, **{'a': 2}))
# The same name in the other message of a `**` argument.
t(lambda: [].sort(**1))
t(lambda: C().m(**1))
t(lambda: C(**1))
