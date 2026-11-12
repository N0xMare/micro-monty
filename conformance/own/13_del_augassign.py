a = [0, 1, 2, 3, 4, 5]
del a[1]
del a[-1]
del a[1:3]
print(a)
d = {"x": 1, "y": 2}
del d["x"]
print(d)
v = 1
del v
try:
    print(v)
except NameError as e:
    print(e)
n = 10
n += 5; n -= 3; n *= 2; n //= 5; n **= 3; n %= 7
print(n)
s = "ab"
s += "cd"; s *= 2
print(s)
l = [1]
l2 = l
l += [2, 3]
print(l, l2, l is l2)
t = (1,)
t2 = t
t += (2,)
print(t, t2)
x = 6
x /= 4
print(x)
x = 0b1010
x &= 0b0110; x |= 0b1000; x ^= 0b0011; x <<= 2; x >>= 1
print(x)
m = [[1, 2], [3, 4]]
m[1][0] += 10
print(m)
try:
    del d["nope"]
except KeyError as e:
    print("KeyError", e)
