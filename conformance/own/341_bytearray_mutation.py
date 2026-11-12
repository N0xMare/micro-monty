# bytearray under sustained mutation (parks and snapshots land between and
# inside these steps), plus user __index__ values and generators as sources.


class I:
    def __init__(self, v):
        self.v = v

    def __index__(self):
        return self.v


b = bytearray()
for i in range(300):
    b.append(i % 256)
    if i % 7 == 0:
        b[i % len(b)] = (i * 3) % 256
    if i % 11 == 0:
        del b[::5]
    if i % 13 == 0:
        b[1:4] = bytes(range(i % 50, i % 50 + 6))
    if i % 17 == 0:
        b.insert(i % (len(b) + 1), 42)
    if i % 19 == 0:
        b += b'xyz'
    if i % 23 == 0 and b:
        b.pop(len(b) // 2)
    if i % 29 == 0:
        b.reverse()
print(len(b), sum(b), bytes(b[:40]).hex())

grow = bytearray(b'ab')
for _ in range(10):
    grow *= 2
grow[100:900] = b''
print(len(grow), grow[:6], grow.count(b'ab'))
grow.clear()
grow.extend(range(200))
print(len(grow), grow[-3:])

c = bytearray(b'abc')
c[I(1)] = I(66)
c[I(0):I(1)] = [I(120), 121]
c[2:2] = (x for x in b'GEN')
c.append(I(33))
print(c, c.find(b'GEN', I(1)))
try:
    c[0] = I(300)
except ValueError as e:
    print('ValueError', e)
