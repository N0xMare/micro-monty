# A bytearray cut to under a quarter of its block, with memory full, keeps
# the block (the smaller one does not fit). A growth that then opens a gap
# below the end, a slice assignment or `insert`, still fails to get a
# smaller block: its tail moves up over itself in the kept block and the
# bytes past the old length are zero, as in a block with spare capacity.
b = bytearray(64 << 20)
b[:16] = b'0123456789abcdef'
m = 8 << 20
b[m - 8:m + 8] = b'ABCDEFGHijklmnop'

# Fill up to 256 MiB, or the memory limit if that is lower.
keep = []
try:
    while len(keep) < 4096:
        keep.append(bytearray(1 << 16))
except MemoryError:
    pass
del b[m:]
print(len(b), b[:12], b[-12:])
b[5:5] = b'xyz'
print(len(b), b[:12], b[-12:])
b.insert(2, 33)
print(len(b), b[:12], b[-12:])
b[1:3] = b'<--->'
print(len(b), b[:12], b[-12:])
b.insert(len(b) - 2, 35)
print(len(b), b[:12], b[-12:])
print(b.count(0), b.find(b'ABCDEFGH'), b.find(b'ijk'))
keep = None
b = None
