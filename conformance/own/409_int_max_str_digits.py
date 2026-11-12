# `sys.set_int_max_str_digits` / `get_int_max_str_digits` / `int_info`:
# the limit the int<->str conversions enforce (CPython's default 4300; 0
# for none) can be raised, lowered and switched off, as the error message
# itself suggests; `sys.flags.int_max_str_digits` follows it.
import sys
print(sys.int_info, sys.flags.int_max_str_digits, sys.get_int_max_str_digits())
x = 7 ** 6000
try:
    str(x)
except ValueError as e:
    print(e)
try:
    int('1' * 5000)
except ValueError as e:
    print(e)
sys.set_int_max_str_digits(maxdigits=6000)
print(sys.flags.int_max_str_digits, sys.get_int_max_str_digits(), len(str(x)), len(str(int('9' * 5000))))
try:
    str(7 ** 8000)
except ValueError as e:
    print(e)
sys.set_int_max_str_digits(0)
print(len(str(7 ** 8000)), f"{7 ** 8000:d}"[:5], sys.flags.int_max_str_digits)
for bad in (5, -1, 'x', 1.5, True):
    try:
        sys.set_int_max_str_digits(bad)
        print('ok', bad)
    except Exception as e:
        print(type(e).__name__, e)
print(sys.get_int_max_str_digits())
sys.set_int_max_str_digits(640)
try:
    print(len(repr(10 ** 640)))
except ValueError as e:
    print(e)
print(len(str(10 ** 639)), hex(10 ** 2000)[:6])
sys.set_int_max_str_digits(4300)
