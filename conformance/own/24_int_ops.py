print(7 // 2, -7 // 2, 7 // -2, 7 % 3, -7 % 3, 7 % -3, divmod(-7, 2))
print(2 ** 10, 2 ** -1, (-2) ** 3, 10 ** 15, 3 ** 0)
print(1 << 40, -1 >> 3, 5 & 3, 5 | 3, 5 ^ 3, ~5, -(-3), +4)
big = 2 ** 47
print(big, big * 2, big * 4 - 1, -big * 2, (big * 4) // 3)
print(140737488355328 + 140737488355328, 9223372036854775807)
print(abs(-5), abs(-2.5), int(3.9), int(-3.9), int("  -42 "), int("ff", 16), int(True))
print(hash(1), hash(-1), hash(0), hash(True), 5 == 5.0, hash(5) == hash(5.0))
print(min(3, 1, 2), max([4, 9, 2]), sum([1, 2, 3], 10), sum([]), min("bca"))
print(round(2.5) if False else "skip", bool(0), bool(-1), 10 / 4, 10 / 5, 7 / 7)
for bad in ("1/0", "1 // 0", "1 % 0", "divmod(1, 0)"):
    pass
try:
    1 // 0
except ZeroDivisionError as e:
    print(e)
try:
    5 % 0
except ZeroDivisionError as e:
    print(e)
try:
    int("12x")
except ValueError as e:
    print(e)
print(True + True, True * 3, -True, 1 + 2.0, 3 * 1.5)
