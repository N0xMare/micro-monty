a = [5, 3, 8, 1]
a.append(7)
a.extend([2, 9])
a.extend(range(2))
print(a, len(a))
print(a.pop(), a.pop(0), a)
a.sort()
print(a)
a.sort(reverse=True)
print(a)
print(a[-1], a[-2:], a[:-2], a[::-2], a[1:5:2], a[10:20], a[-100:2])
b = a[:]
b[0] = 100
print(a[0], b[0])
print([1, 2] + [3], [0] * 3, [[0]] * 2, 3 in a, 42 in a, a.index(8), a.count(1))
a.insert(0, -1)
a.remove(8)
a.reverse()
print(a)
a[1:3] = ["x", "y", "z"]
print(a)
print(sorted(["b", "a", "C"]), sorted([3, 1, 2], reverse=True), list("abc"), list(range(3)))
print(list(reversed([1, 2, 3])), [] == [], [1, 2] == [1, 2], [1] != [2])
nested = [1, [2, [3, []]]]
print(nested, len(nested))
for bad in (lambda: a[99], lambda: [].pop(), lambda: a.index(1234), lambda: [1, "a"].sort()):
    try:
        bad()
    except (IndexError, ValueError, TypeError) as e:
        print(type(e).__name__, e)
x = [1, 2, 3]
x[-1] = 30
print(x, x.copy(), x.clear(), x)
# Repeating an empty sequence a huge number of times copies nothing.
e = []
e *= 2**62
print([] * 2**62, 2**62 * (), '' * 2**62, b'' * 2**62, e, [1, 2] * 0, (3,) * -5)
