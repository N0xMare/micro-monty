vals = [0.1, 0.2, 0.1 + 0.2, 1.0, -0.0, 1e16, 1e15, 123456789012345678.0, 1e-4, 1e-5,
        0.00001234, 1.5e300, 2.0 ** 70, 1 / 3, 2 / 3, 100.0, 3.14159, 1e22, 1e-7, 5e-324,
        1.7976931348623157e308, 12345.678, 0.5, -1.25e-10, 9007199254740993.0]
for v in vals:
    print(repr(v), str(v))
print(float("inf"), float("-inf"), float("nan"), float(" 2.5 "), float(3))
print(1e16 == 10 ** 16, 0.1 + 0.2 == 0.3, 3.0 == 3, float("1e500"))
print(7.0 // 2, -7.0 // 2, 7.5 % 2, -7.5 % 2, 2.0 ** 0.5, 10.0 ** -2, divmod(7.5, 2))
print(int(1e15), abs(-0.0), [1.0, 2.5], (0.1,), {1.5: 2.0})
try:
    1.0 / 0
except ZeroDivisionError as e:
    print(e)
try:
    2.0 % 0.0
except ZeroDivisionError as e:
    print(e)
