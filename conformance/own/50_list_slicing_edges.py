l = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
print(l[::3], l[1::2], l[-3:], l[:-3], l[::-1], l[8:2:-2], l[-1:-11:-3], l[100:], l[-100:-50])
l2 = l[:]
l2[2:5] = []
print(l2)
l2[1:1] = [99, 98]
print(l2)
del l2[::2]
print(l2)
m = [3, 1, 2]
m.sort()
print(m, sorted(m, reverse=True), [x for x in reversed(m)])
words = ["b", "A", "a", "B"]
print(sorted(words), sorted(words, reverse=True))
nested = [[3, 1], [1, 2], [1, 1]]
nested.sort()
print(nested)
x = [1, 2]
x.append(x)
print(x)
d = {"a": 1}
d["self"] = d
print(d)
print([1, 2, 3] * 0, [] * 5, [None] * 2, [[]] * 2)
print([1] + [], [] + [], list(range(3)) + list("ab"))
a = [1, 2, 3]
b = a
a = a + [4]
print(a, b)
c = [0] * 3
c[0] += 5
print(c, c.pop(-1), c)
try:
    [1, 2].pop(5)
except IndexError as e:
    print(e)
try:
    l[1:2:0]
except ValueError as e:
    print(e)
try:
    [1, 2, 3][float(1)]
except TypeError as e:
    print(e)
try:
    l["x"]
except TypeError as e:
    print(e)

# Extreme strides still select only valid positions. Advancing past the final
# selected item must not overflow, nor may taking the magnitude of the stride.
for step in [-(1 << 63), -(1 << 63) + 1, -(1 << 62), (1 << 63) - 1, (1 << 63) - 2, 1 << 62]:
    for seq, expected in [([1, 2, 3], [2]), ((1, 2, 3), (2,)), ('abc', 'b'), (b'abc', b'b')]:
        assert seq[1::step] == expected
    items = [1, 2, 3]
    items[1::step] = [9]
    assert items == [1, 9, 3]
    del items[1::step]
    assert items == [1, 3]
    assert [1, 2, 3][0:0:step] == []
assert [1, 2, 3][::-(1 << 63)] == [3]
assert b'abc'[::-(1 << 63)] == b'c'
