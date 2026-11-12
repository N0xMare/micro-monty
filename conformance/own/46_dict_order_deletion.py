d = {}
for i in range(10):
    d[str(i)] = i
for k in ["3", "0", "9"]:
    del d[k]
d["0"] = "re-added"
d["5"] = "updated"
print(list(d.items()))
print(d.pop("1"), list(d))
e = dict.fromkeys("abc", 0) if False else {"a": 0, "b": 0, "c": 0}
del e["a"]
e["a"] = 1
print(e, list(e.keys())[0])
