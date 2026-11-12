# Straight-line long operators: each one after another stops for a
# safepoint before it starts (C4 granularity, docs/spec.md §8.6), and runs
# again from there unchanged, or parks there.
s = "ab" * 50_000
t = "ab" * 49_999 + "ba"
b1 = s == t
b2 = s < t
b3 = "ba" in s
b4 = len(s + t)
b5 = s != t
x = 3 ** 50_000
y = -x
z = ~x
b6 = f"{list(range(3_000))}"[-12:]
b7 = f"{x:x}"[:5]
print(b1, b2, b3, b4, b5, y == -x, z == -x - 1, b6, b7)
try:
    q = s < 1
except TypeError as e:
    print("TypeError:", e)
u = s
u += t
u += s
print(len(u), u[-3:], s * 2 == s + s, (x * 2) % 1_000_007)
