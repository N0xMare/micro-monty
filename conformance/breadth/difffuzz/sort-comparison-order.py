# list.sort/sorted compare in a different order from CPython's timsort
# (binary insertion), so the first failing pair differs: CPython reports
# 'str' and 'int', mm 'int' and 'str'. The same order difference changes the
# result for partial orders (e.g. sets) and the calls a user __lt__ sees.
try:
    sorted([0, 0, 'a', 0])
except TypeError as e:
    print(e)
print([sorted(s) for s in sorted([frozenset(), frozenset({0, 3}), frozenset({1, 2, 3}), frozenset({0, 2, 3}), frozenset({1, 2})])])
