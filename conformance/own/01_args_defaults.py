def f(a, b=2, *args, c, d=4, **kw):
    return (a, b, args, c, d, sorted(kw.items()))

print(f(1, c=3))
print(f(1, 5, 6, 7, c=3, e=9, d=0))
print(f(*[1, 2, 3], **{"c": 10, "z": 11}))

def g(x, y=[]):
    y.append(x)
    return y

print(g(1), g(2))

def h(a, /, b, *, c=3):
    return a + b + c

print(h(1, 2), h(1, b=2, c=10))
print((lambda x, y=10: x * y)(4))
print((lambda *a, **k: (a, k))(1, 2, z=3))

# Python permits more defaults than positional parameters. They align from
# the right, but excess-argument errors still report the signed minimum.
def changed(a, /, b=2, *, c=30):
    return a, b, c

for ds in [(20,), (10, 20), (5, 10, 20), (), None]:
    changed.__defaults__ = ds
    for args, kwargs in [
        ((), {}), ((7,), {}), ((), {"b": 8}),
        ((7, 8), {"c": 9}), ((7, 8, 9), {"c": 9}), ((7,), {"a": 8}),
    ]:
        try:
            print(changed(*args, **kwargs))
        except TypeError as e:
            print(e)

def no_positional():
    return 42

no_positional.__defaults__ = (4, 5)
print(no_positional())
try:
    no_positional(1)
except TypeError as e:
    print(e)

def variadic(a=1, *rest, c=3):
    return a, rest, c

variadic.__defaults__ = (11, 22, 33)
print(variadic(), variadic(8), variadic(8, 9, 10, c=4))
