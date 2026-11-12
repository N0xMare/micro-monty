# int / int is correctly rounded, also when the quotient is subnormal, tiny
# enough to round to zero, or close to the float range's top.
print(6 / (int(1e300) << 30))
print(-6 / (int(1e300) << 30))
big = int(1e300) << 30
for a in (1, 3, 5, 6, 7, 2 ** 60 + 1, 3 ** 40):
    print(a / big, a / (big << 40), a / (big << 200), -a / (big << 2000))
for k in (1073, 1074, 1075, 1076):
    print(1 / 2 ** k, 3 / 2 ** (k + 1), (2 ** 53 + 1) / 2 ** (k + 53))
x = 0
for i in range(1, 400):
    q = (i * 7919 ** 41) / (13 ** 300 * i + 1)
    x = (x * 31 + int(repr(q).replace('.', '').replace('e-', '')[:12])) % 1000003
print(x)
print(2 ** 1024 // 3 / 1, (2 ** 1024 - 2 ** 971) / 1)
for a, b in ((2 ** 1025, 1), (2 ** 1024, 1), (2 ** 1024 - 2 ** 970, 1), (10 ** 400, 10 ** 70)):
    try:
        print(a / b)
    except OverflowError as e:
        print("OverflowError:", e)
