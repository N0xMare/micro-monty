# bytearray: a buffer built by append, another by += of a chunk with item
# writes, then index reads, a stepped slice and sum.
buf = bytearray()
for i in range(600_000):
    buf.append(i * 7 & 255)
chunk = bytearray(b'abcdefgh')
grown = bytearray()
for i in range(80_000):
    grown += chunk
    grown[-1] = i & 127
total = 0
for i in range(0, len(buf), 3):
    total += buf[i]
print(len(buf), total, len(grown), sum(grown[::7]))
