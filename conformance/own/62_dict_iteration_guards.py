d = {"a": 1, "b": 2, "c": 3, "d": 4}
seen = []
try:
    for k in d:
        seen.append(k)
        if k == "b":
            d.pop("a")
            d["z"] = 9
except RuntimeError as exc:
    print(exc, seen)
d = {"a": 1, "b": 2, "c": 3}
seen = []
for k in d:
    seen.append(k)
    if k == "a":
        d.pop("c")
        d["z"] = 9
print(seen)
d = {"a": 1, "b": 2, "c": 3}
it = iter(d)
next(it)
d.pop("a")
try:
    next(it)
except RuntimeError as exc:
    print(exc)
print(list(it))
s = {1, 2, 3}
try:
    for x in s:
        s.add(x + 10)
except RuntimeError as exc:
    print(exc)
it = iter([1, 2, 3])
print(2 in it, list(it))
print(("a", 1) in {"a": 1}.items(), ("a", 2) in {"a": 1}.items(), ("a",) in {"a": 1}.items())
try:
    ([1], "x") in {1: "x"}.items()
except TypeError as exc:
    print(exc)
try:
    try:
        raise TypeError("t")
    except ((ValueError,),):
        pass
except TypeError as exc:
    print(exc)
def k1(a, *, b=1):
    return a
try:
    k1(1, 2, bad=3)
except TypeError as exc:
    print(exc)
print(10.0 ** 2, 2.0 ** 0.5, 4 ** 0.5, 9.0 ** -0.5)
try:
    10.0 ** 400
except OverflowError as exc:
    print("OverflowError", exc)
