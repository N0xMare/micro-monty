d = {"b": 1, "a": 2}
d["c"] = 3
d["b"] = 10
print(d, len(d), list(d), list(d.keys()), list(d.values()), list(d.items()))
del d["b"]
d["b"] = 20
print(d)
print(d.get("a"), d.get("zz"), d.get("zz", 0), "a" in d, "zz" not in d)
print(d.pop("a"), d.pop("nope", "default"), d)
d.update({"x": 1}, y=2)
print(d, d.setdefault("x", 5), d.setdefault("w", 6), d)
e = {1: "int", (1, 2): "tuple", "1": "str", 2.0: "float"}
print(e[1], e[(1, 2)], e["1"], e[2], e[1.0], e[True])
for k, v in e.items():
    print(k, v)
print(dict(a=1, b=2), dict([("k", "v")]), dict(), dict({"z": 0}))
print({} == {}, {"a": 1} == {"a": 1}, {"a": 1, "b": 2} == {"b": 2, "a": 1})
try:
    d["missing"]
except KeyError as err:
    print("KeyError", err, repr(err))
try:
    {[1]: 2}
except TypeError as err:
    print(err)
big = {i: i * i for i in range(20)}
for i in range(0, 20, 3):
    del big[i]
big[0] = "back"
print(big)
print(list(reversed(list(big.keys())))[:3], d.copy())
