s = {3, 1, 2, 3}
print(s, len(s), 2 in s, 5 in s)
s.add(10)
s.discard(1)
s.remove(2)
print(sorted(s), set(), set("aab") == {"a", "b"}, {1, 2} | {3}, {1, 2} & {2}, {1, 2} - {1}, {1, 2} ^ {2, 3})
print(sorted({x % 4 for x in range(20)}), {1, 2} <= {1, 2, 3}, {1} == {1})
try:
    s.remove(99)
except KeyError as e:
    print("KeyError", e)
try:
    {[1]}
except TypeError as e:
    print(e)
print({"a"}, {(1, 2)}, set([1]))
