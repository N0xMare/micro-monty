def deep(n):
    if n == 0:
        raise ValueError("bottom")
    return deep(n - 1)
try:
    deep(50)
except ValueError as e:
    print("caught deep", e)

def kw(a, *, b, **rest):
    return a, b, rest
print(kw(1, b=2, c=3, d=4))
try:
    kw(1, 2)
except TypeError as e:
    print(e)
try:
    kw(1)
except TypeError as e:
    print(e)
def two_missing(a, b, c):
    pass
try:
    two_missing()
except TypeError as e:
    print(e)
try:
    two_missing(1)
except TypeError as e:
    print(e)
def kw2(*, a, b):
    pass
try:
    kw2()
except TypeError as e:
    print(e)
def defaults(a, b=1, c=2):
    pass
try:
    defaults(1, 2, 3, 4)
except TypeError as e:
    print(e)
try:
    defaults()
except TypeError as e:
    print(e)
try:
    defaults(1, 2, b=3)
except TypeError as e:
    print(e)
def onlypos(a, b, /):
    pass
try:
    onlypos(1, b=2)
except TypeError as e:
    print(e)
f = lambda: 0
try:
    f(1)
except TypeError as e:
    print(e)
try:
    (lambda x: x)()
except TypeError as e:
    print(e)
def star(*a):
    return a
print(star(), star(*[]), star(*range(3), 4), star(*"ab", *[1]))
def kws(**k):
    return k
print(kws(), kws(**{"a": 1}, b=2))
try:
    kws(**{"a": 1}, a=2)
except TypeError as e:
    print(e)
try:
    kws(**[1])
except TypeError as e:
    print(e)
try:
    star(*1)
except TypeError as e:
    print(e)
one, abc = 1, "abc"
try:
    one()
except TypeError as e:
    print(e)
try:
    abc()
except TypeError as e:
    print(e)
try:
    print(sep=1)
except TypeError as e:
    print(e)
try:
    len(x=1)
except TypeError as e:
    print(e)
print(int("5", base=10), sorted([2, 1], reverse=False))
try:
    sorted([1], key=None, foo=1)
except TypeError as e:
    print(e)
