# A positional-only parameter passed by keyword, to a function without
# `**kwargs`, raises CPython's "got some positional-only arguments passed
# as keyword arguments" with every such name, in parameter order, ahead
# of an unexpected keyword or too many positionals; with `**kwargs` the
# name goes into the mapping.


def f(a, b, /, c=1, *, d=2):
    pass


def k(a=1, /, *args, **kw):
    return a, args, kw


class C:
    def m(self, x, /, y=0):
        pass


def err(call):
    try:
        call()
    except TypeError as e:
        return str(e)


posonly = "f() got some positional-only arguments passed as keyword arguments: "
assert err(lambda: f(1, b=2)) == posonly + "'b'"
assert err(lambda: f(b=1, a=2)) == posonly + "'a, b'"
assert err(lambda: f(1, z=3, b=2)) == posonly + "'b'"
assert err(lambda: f(1, 2, 3, 4, b=5)) == posonly + "'b'"
assert err(lambda: f(1, 2, c=3, z=1)) == "f() got an unexpected keyword argument 'z'"
assert err(lambda: f(1, 2, 3, c=4, b=1)) == "f() got multiple values for argument 'c'"
assert err(lambda: C().m(y=1, x=1)) == (
    "C.m() got some positional-only arguments passed as keyword arguments: 'x'"
)
assert err(lambda: (lambda a, b, /: 0)(1, a=1, b=2)).endswith(": 'a, b'")
assert k(a=5) == (1, (), {'a': 5}) and k(1, 2, a=3) == (1, (2,), {'a': 3})
