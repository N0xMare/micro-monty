t = (1, 2, 3)
print(t, t[0], t[-1], t[1:], len(t), t + (4,), t * 2, 2 in t, (), (1,), tuple([1, 2]), tuple("ab"))
print(t.index(2), t.count(3), (1, 2) == (1, 2), (1, (2, 3)), hash((1, 2)) == hash((1, 2)))
r = range(10)
print(r, range(1, 5), range(0, 10, 3), len(r), r[3], r[-1], list(range(5, 0, -2)), 3 in r, 10 in r)
print(list(range(0)), list(range(-3)), len(range(0, 100, 7)), range(3) == range(3))
print(r[2:5], list(r[::3]))
try:
    t[5]
except IndexError as e:
    print(e)
try:
    t[0] = 1
except TypeError as e:
    print(e)
try:
    range(1, 2, 0)
except ValueError as e:
    print(e)
x = 1, 2
print(x, type(x))
print(min((3, 1), (2, 5)), max([(1, "b"), (1, "a")]))
