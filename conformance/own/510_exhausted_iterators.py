# An exhausted builtin iterator drops its sequence, as CPython's do: it
# stays exhausted when the sequence grows again (a bytearray's used to
# read the new bytes), and holds no memory (tests/session.rs).
a = [1, 2]
it = iter(a)
print(list(it), next(it, 'end'))
a.append(3)
print(list(it), a)
b = bytearray(b'ab')
it = iter(b)
print(list(it))
b.append(99)
print(list(it), next(it, 'end'), b)
for make in (lambda x: iter(x), lambda x: reversed(x), lambda x: iter(tuple(x)), lambda x: iter(dict.fromkeys(x)),
             lambda x: iter(set(x)), lambda x: iter(bytes(x)), lambda x: iter(''.join(map(str, x))), lambda x: iter('é' * len(x))):
    it = make([1, 2, 3])
    print(list(it), list(it), next(it, 'end'), type(it).__name__)
a = [1, 2, 3]
it = iter(a)
next(it)
a.clear()
print(list(it))
a.extend([4, 5, 6])
print(list(it), a)
a = [1, 2]
r = reversed(a)
print(list(r))
a.append(3)
print(list(r))
# Read again before the end, an iterator sees what changed.
a = [1, 2]
it = iter(a)
print(next(it))
a.append(3)
print(list(it))
