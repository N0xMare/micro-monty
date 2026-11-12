# sys attributes, set/dict methods added with M5, round() and int() with
# huge arguments.
import sys


def attempt(f):
    try:
        print(repr(f()))
    except Exception as e:
        print(type(e).__name__, e)


print(sys.version_info[:2], sys.version_info.major, sys.version_info >= (3, 8), len(sys.version_info))
print(sys.maxsize, sys.maxunicode, sys.byteorder, sys.float_info.max, sys.float_info.epsilon)
print(sys.flags.int_max_str_digits, isinstance(sys.platform, str), __debug__)
print(sys.getrecursionlimit() > 50)
sys.setrecursionlimit(100)
print(sys.getrecursionlimit())


def down(n):
    return 0 if n == 0 else down(n - 1)


attempt(lambda: down(200))
print(down(50))
attempt(lambda: sys.setrecursionlimit(0))
attempt(lambda: sys.setrecursionlimit(1.5))
attempt(lambda: sys.version_info.nope)

s = {1, 2, 3}
print(s.symmetric_difference([3, 4]), s.isdisjoint([9]), s.isdisjoint((1,)))
s.symmetric_difference_update({3, 5})
print(s)
s.intersection_update([1, 2, 7], {1, 2})
print(s)
s.difference_update([1], [9])
print(s)
print(dict.fromkeys('abc'), dict.fromkeys([1, 2], 0), {}.fromkeys(iter([5])))
attempt(lambda: dict.fromkeys())
attempt(lambda: s.symmetric_difference(1))

print(round(5, 10**30), round(-5, 10**30), round(2.675, 10**30), round(1.5, -(10**30)))
print(repr(round(-1.5, -(10**30))), round(12345, -(10**5)), round(2**70, -100), round(2**70, 2**70))
attempt(lambda: int('1', 10**20))
attempt(lambda: int('1', -(10**20)))
