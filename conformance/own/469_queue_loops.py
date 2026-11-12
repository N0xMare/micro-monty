# Queue loops: `set.pop()` starts past the slot it last took (CPython's
# finger), so its order follows CPython's across pops and adds, and a
# `pop()` loop is linear; `list.pop(i)`, `insert(i, x)` and `del l[i]`
# move the tail in place. Long lists cross the chunked rebuild that
# `insert` takes when the buffer is full.

s = set(range(20))
out = [s.pop() for _ in range(5)]
s.add(3)
out += [s.pop() for _ in range(3)]
print(out, sorted(s))
s = {1, 2, 3}
print(s.pop(), s.pop())
s.add(1)
s.add(2)
print(s.pop(), s.pop(), s.pop(), s)
try:
    s.pop()
except KeyError as e:
    print('KeyError', e)

# A finger past the end wraps to the start of the table.
w = set(range(8))
print([w.pop() for _ in range(7)])
w.add(0)
print(w.pop(), w.pop(), w)

# A long pop() loop.
big = set(range(100_000))
n = 0
while big:
    big.pop()
    n += 1
print(n, big)
c = set(range(10))
c.pop()
c.clear()
c.update([5, 6, 7])
print(c.pop(), sorted(c))

# A list as a queue, short and long.
q = list(range(50))
while len(q) > 3:
    q.pop(0)
    q.insert(1, q.pop())
print(q)
L = 40_000
xs = list(range(L))
for i in range(5):
    xs.pop(0)
    xs.insert(3, -i)
    del xs[100]
xs.insert(-5, 'x')
xs.insert(10**9, 'end')
xs.insert(-10**9, 'start')
print(xs[:10], xs[-8:], len(xs))
ys = []
for i in range(L):
    ys.insert(0, i)
print(len(ys), ys[:3], ys[-3:])
total = 0
while ys:
    total += ys.pop(0)
print(total, ys)
o = [object() for _ in range(10)]
a = o[3]
o.pop(3)
print(a in o, len(o))
