print([x * y for x in range(1, 4) for y in range(x) if (x + y) % 2])
print({k: v for k, v in zip("abc", range(3))})
print({x % 3 for x in range(10)})
print(sum(x for x in range(101)))
print(list(c for c in "xyz"))
g = (x * x for x in range(4))
print(next(g), next(g), list(g))
x = "outer"
print([x for x in range(3)], x)
fs = [lambda: z for z in range(3)]
print([f() for f in fs])
print([[j for j in range(i)] for i in range(4)])
n = 3
print([n * k for k in range(n)])
def f():
    m = 10
    return [m + i for i in range(3)], {i: m for i in range(2)}
print(f())
print(any(v > 2 for v in [1, 2, 3]), all(v > 0 for v in []))
