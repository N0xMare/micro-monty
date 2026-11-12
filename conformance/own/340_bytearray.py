# bytearray: constructors, mutation, shared bytes methods, comparisons,
# hashing and struct.pack_into, with CPython 3.14's results and messages.
import struct


def err(f, *a):
    try:
        r = f(*a)
    except Exception as e:
        print(type(e).__name__, e)
    else:
        print('ok', r)


# Constructors.
print(bytearray(), bytearray(3), bytearray(b'ab'), bytearray([1, 2, 255]))
print(bytearray('hé', 'utf-8'), bytearray('hi', encoding='ascii', errors='strict'))
print(bytearray(x for x in range(3)), bytearray(bytearray(b'q')), bytearray(range(65, 68)))
print(bytearray.fromhex('41 42'), bytearray(b'z').fromhex('00ff'), type(bytearray.fromhex('')))
err(bytearray, -1)
err(bytearray, 1.5)
err(bytearray, [256])
err(bytearray, ['a'])
err(bytearray, 'abc')
err(bytearray, b'x', 'utf-8')
err(bytearray, None)
print(bytes(bytearray(b'xy')), type(bytes(bytearray())), list(bytearray(b'AB')))

# Indexing and slicing.
b = bytearray(b'hello world')
print(b[0], b[-1], b[1:4], b[::2], b[::-1], b[100:], type(b[:]), b[:] is b)
err(b.__getitem__, 11)
err(b.__getitem__, 'x')

# Item and slice assignment.
b[0] = 72
b[-1] = ord('D')
print(b)
b[0:5] = b'HOWDY'
print(b)
b[5:5] = b',,'
print(b)
b[0:7] = b''
print(b)
b[1:3] = [65, 66, 67, 68]
print(b)
b[::2] = b'xxxx'
print(b)
b[3:1] = b'!'
print(b)
b[:] = b
print(b)
b[1:2] = b
print(b, len(b))
b[::3] = b''
print(b)
b[2:2] = (x for x in b'gen')
print(b)
err(b.__setitem__, 0, 256)
err(b.__setitem__, 0, 'a')
err(b.__setitem__, 99, 1)
err(b.__setitem__, slice(0, 1), 5)
err(b.__setitem__, slice(0, 1), 'ab')
err(b.__setitem__, slice(0, 1), [300])
err(b.__setitem__, slice(None, None, 2), b'ab')
err(b.__setitem__, 'k', 1)

# Deletion.
c = bytearray(b'0123456789')
del c[0]
del c[-1]
print(c)
del c[1:3]
print(c)
del c[::2]
print(c)
c = bytearray(range(20))
del c[::-3]
print(list(c))
del c[5:2]
print(len(c))
err(c.__delitem__, 50)

# Methods that mutate.
d = bytearray()
d.append(1)
d.append(True)
d.extend(b'ab')
d.extend([3, 4])
d.extend(bytearray(b'Z'))
d.extend(x for x in (5, 6))
d.extend(d)
print(d)
d.insert(0, 9)
d.insert(-1, 8)
d.insert(100, 7)
d.insert(-100, 6)
print(d)
print(d.pop(), d.pop(0), d.pop(-2), d)
d.remove(97)
print(d)
d.reverse()
print(d)
e = d.copy()
d.clear()
print(d, e, len(d), bool(d), bool(e))
err(d.pop)
err(e.pop, 99)
err(e.remove, 200)
err(e.remove, 300)
err(e.append, 256)
err(e.append, 'a')
err(e.extend, 1)
err(e.extend, 'ab')
err(e.extend, [1, 'a'])
err(e.insert, 0, -1)

# Operators.
f = bytearray(b'ab')
g = f
f += b'cd'
f += bytearray(b'e')
f.extend(f)
print(f, g is f)


def iadd(x, y):
    x += y
    return x


err(iadd, f, f)
print(iadd(bytearray(), bytearray()))
f *= 2
print(f, g is f, len(f))
f *= 0
print(f, g is f)
print(bytearray(b'x') + b'y', b'x' + bytearray(b'y'), type(b'x' + bytearray(b'y')))
print(bytearray(b'ab') * 3, 2 * bytearray(b'c'), type(bytearray(b'z') * 2))
err(lambda: bytearray(b'a') + 'b')
err(lambda: bytearray(b'a') + 1)
err(lambda: b'a' + 1)
h = bytearray(b'q')
err(iadd, h, 5)
err(iadd, h, 'x')
print(97 in bytearray(b'abc'), b'bc' in bytearray(b'abc'), bytearray(b'c') in b'abc')
print(bytearray(b'') in bytearray(b'x'), 300 in bytearray(b'x') if False else 'skip')

# Comparisons and hashing.
print(bytearray(b'abc') == b'abc', b'abc' == bytearray(b'abc'), bytearray(b'a') != b'a')
print(bytearray(b'abc') < b'abd', b'b' > bytearray(b'a'), bytearray(b'a') <= bytearray(b'a'))
print(bytearray(b'a') == 'a', bytearray() == b'', [bytearray(b'x')] == [b'x'])
print(sorted([bytearray(b'b'), b'a', bytearray(b'c')]))
err(hash, bytearray(b'a'))
err(lambda: {bytearray(): 1})
err(lambda: {bytearray()})
err(lambda: bytearray(b'a') < 'a')

# Shared bytes methods return bytearrays.
s = bytearray(b'  Hello, World  ')
for r in (s.strip(), s.lower(), s.upper(), s.split(), s.split(b','), s.replace(b'l', b'L'),
          s.partition(b','), s.rpartition(b'?'), s.center(20, b'*'), s.zfill(3),
          bytearray(b',').join([b'a', bytearray(b'b')]), s.splitlines(), s.title(),
          s.removeprefix(b'  '), s.ljust(2), s[2:7].swapcase()):
    print(type(r).__name__, r)
print(s.find(b'o'), s.rfind(111), s.count(b'l'), s.index(b'W'), s.startswith(b'  H'),
      s.endswith((b'x', b'  ')), s.isascii(), s.strip().isalpha(), s.hex(), s.hex(':', 2))
print(s.decode(), str(bytearray(b'abc'), 'ascii'), bytearray(b'\xff').decode('latin-1'))
print(b'abc'.find(bytearray(b'c')), b'a-b'.split(bytearray(b'-')), b''.join([bytearray(b'x'), b'y']))
p = bytearray(b'abc')
q = p.partition(b'b')
print(q, q[1] is p, p.zfill(1) is p, p.ljust(1) is p)
print(int(bytearray(b'42')), float(bytearray(b'1.5')), int.from_bytes(bytearray(b'\x01\x00'), 'little'))
print(repr(bytearray(b"it's \"q\" \n\x00")), str(bytearray(b'x')), f'{bytearray(b"y")}')
print([repr(bytearray(x)) for x in (b"'", b'"', b"'\"", b'\\')])
print(type(iter(bytearray())).__name__, list(reversed(bytearray(b'abc'))), sum(bytearray(b'\x01\x02')))
it = iter(bytearray(b'ab'))
print(next(it))
print(isinstance(bytearray(), bytes), isinstance(bytearray(), bytearray), type(bytearray()).__name__)
print(issubclass(bytearray, object), bytearray.__name__, callable(bytearray.append))

# Iteration sees mutation.
m = bytearray(b'abc')
out = []
for x in m:
    out.append(x)
    if len(m) < 6:
        m.append(x + 1)
print(out, m)

# struct with bytearrays.
buf = bytearray(8)
struct.pack_into('<hI', buf, 1, -2, 0xDEADBEEF)
print(buf, struct.unpack_from('<hI', buf, 1), struct.unpack('<8B', buf))
struct.Struct('>H').pack_into(buf, -2, 513)
print(buf, list(struct.iter_unpack('<H', buf)))
err(struct.pack_into, 'i', b'abcd', 0, 1)
err(struct.pack_into, 'i', buf, 6, 1)
err(struct.pack_into, 'i', buf, -2, 1)
err(struct.pack_into, 'i', buf, -20, 1)
err(struct.pack_into, 'ii', buf, 0, 1)
err(struct.pack_into, 'i', buf)
print(struct.pack('3s', bytearray(b'xyzw')), struct.pack('c', b'!'))
err(struct.pack, 'c', bytearray(b'!'))

# Bytes formatting takes bytearray arguments.
print(b'%s|%b' % (bytearray(b'a'), bytearray(b'b')))
