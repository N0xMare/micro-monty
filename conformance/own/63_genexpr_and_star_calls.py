def f(*args, **kwargs):
    return args, sorted(kwargs.items())
print(f(*(i for i in range(3)), **{"k": 1}))
print(f(1, *[2, 3], 4, *(), x=1, **{"y": 2}))
print(max(x * 2 for x in [3, 1, 2]), min((x for x in "bca")), tuple(x for x in "ab"))
print(dict((k, v) for k, v in [("a", 1)]), sum(x for x in range(4)), sorted(x for x in {3, 1}))
print(any(x for x in []), list(x for x in range(0)))
def g(a, b, c=0):
    return a + b + c
args = (1, 2)
print(g(*args), g(*args, c=3), g(*[1], **{"b": 5}))
try:
    g(*[1, 2, 3, 4])
except TypeError as e:
    print(e)
try:
    g(**{"a": 1, "b": 2}, a=3)
except TypeError as e:
    print(e)
try:
    g(*None)
except TypeError as e:
    print(e)
try:
    [*None]
except TypeError as e:
    print(e)
try:
    {*None}
except TypeError as e:
    print(e)
