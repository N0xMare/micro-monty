d = {"one": 1, "two": 2, "three": 3}
for k in d:
    print(k, end=" ")
print()
for v in d.values():
    print(v, end=" ")
print()
s = "".join(k[0] for k in d.keys())
print(s)
it = iter(range(3))
for x in it:
    print("x", x, next(it, None))
print([i for i, _ in enumerate([5, 6, 7]) if i])
pairs = list(zip(range(3), "abc", [True, False, None]))
print(pairs)
nested = [[1, 2], [3], []]
flat = []
for sub in nested:
    for item in sub:
        flat.append(item)
print(flat)
count = 0
for i in range(1000):
    for j in range(10):
        count += 1
print(count)
try:
    for z in 5:
        pass
except TypeError as e:
    print(e)
try:
    for k in d:
        d["new"] = 1
except RuntimeError as e:
    print(e)
