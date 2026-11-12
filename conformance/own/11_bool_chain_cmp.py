print(1 < 2 < 3, 1 < 3 < 2, 3 > 2 == 2, 1 == 1.0 != 2)
print(0 or "", 0 or "x", 1 and [], [] and 1, None or 0 or "last")
print(not 0, not [1], not None)
def side(v):
    print("side", v)
    return v
print(side(1) < side(2) < side(0) < side(5))
print(side(0) and side(1), side(2) or side(3))
x = 5
print(0 < x <= 5, "a" < "b" < "c", (1, 2) < (1, 3), [1, 2] < [1, 2, 0])
print(None is None, [] is not [], 3 in [1, 2, 3], "b" not in "abc")
