# bytes equality between unequal lengths that share a prefix, at and around
# the sizes an equality compare may read in fixed chunks (32 bytes on musl).
x = b'a' * 32
y = b'a' * 64
print(x == y, y == x, x != y, [x] == [y], x in [y], bytearray(x) == y, y == bytearray(x))
m = b'\xff' * 64
n = b'\xff' * 96
print(m == n, [m, 1] == [n, 1], (m,) == (n,), {m: 1} == {n: 1}, m in {n}, n in {m: 1})
print(bytearray(m) == bytearray(n), x == b'', b'' == x, b'' == b'')
for size in [0, 1, 31, 32, 33, 63, 64, 65, 127, 128, 129]:
    p = b'q' * size
    print(size, [p == b'q' * k for k in (0, 1, 31, 32, 33, 64, 65, 96, 128)], p == p + b'', p == p + b'q')
print(b'a' * 64 + b'b' == b'a' * 64 + b'c', b'a' * 65 == b'a' * 64 + b'b')
print(b'b' + b'a' * 64 == b'c' + b'a' * 64, b'a' * 31 + b'b' + b'a' * 32 == b'a' * 64)

# str and int equality take the same compare
s = 'a' * 32
t = 'a' * 64
print(s == t, [s] == [t], s in [t], s in {t}, 'é' * 16 == 'é' * 32, 'é' * 32 == 'é' * 32)
i = 1 << 256
j = 1 << 512
print(i == j, [i] == [j], i in {j}, (i << 256) == j, i - 1 == (1 << 256) - 1)
