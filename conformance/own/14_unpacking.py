a, b = 1, 2
a, b = b, a
print(a, b)
(c, d), e = (3, 4), 5
print(c, d, e)
first, *rest = [1, 2, 3, 4]
print(first, rest)
*init, last = "hello"
print(init, last)
h, *mid, t = range(5)
print(h, mid, t)
x, *y, z = 1, 2
print(x, y, z)
for i, (k, v) in enumerate({"a": 1, "b": 2}.items()):
    print(i, k, v)
[p, q] = [7, 8]
print(p, q)
def bad1():
    a, b = [1, 2, 3]
def bad2():
    a, b, c = [1, 2]
def bad3():
    a, *b, c = [1]
def bad4():
    a, b = 5
for f in (bad1, bad2, bad3, bad4):
    try:
        f()
    except (ValueError, TypeError) as ex:
        print(type(ex).__name__, ex)
print([*range(3), *"ab"], (*[1], 2), {**{"a": 1}, "b": 2})
