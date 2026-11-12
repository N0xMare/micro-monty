print("a", "b", sep="-", end="!\n")
print()
print(1, 2, 3, sep="")
print("no newline", end="")
print(" continued")
print(len([1, 2]), len("héllo"), len({}), len(range(3, 10)))
print(list(enumerate("ab")), list(enumerate("ab", 1)), list(zip([1, 2, 3], "ab")), list(zip()))
print(any([]), any([0, 1]), all([1, 1]), all([1, 0]))
print(sorted({3: 1, 1: 2}), sorted("hello"), sorted((2, 1)))
print(bool(""), bool("x"), bool([]), bool([0]), bool(None), bool(0.0))
print(str(), int(), float(), list(), tuple(), dict(), bool())
print(isinstance(True, int), isinstance(1, bool), isinstance("s", (int, list)), isinstance([], list))
print(type(1) == int, type("") is str, type(1.0).__name__, int.__name__)
it = iter([10, 20])
print(next(it), next(it), next(it, "done"))
try:
    next(it)
except StopIteration as e:
    print("StopIteration", repr(e))
print(id(it) == id(it), hash("abc") == hash("abc"), repr(None), repr(True))
print(min([1]), max(1, 2, 3), min("a", "b"), sum([0.1] * 10), sum(range(5), 0.5))
for bad in (lambda: min([]), lambda: max(), lambda: sum(["a"]), lambda: int(None), lambda: len(5), lambda: iter(5), lambda: next([])):
    try:
        bad()
    except (ValueError, TypeError) as e:
        print(type(e).__name__, e)
