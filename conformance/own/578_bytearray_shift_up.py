# A bytearray with spare capacity opens a gap in place: its tail moves up
# over itself. Every tail length around 32 and 4096 bytes with every gap
# around them, against the same splice made from parts.
import hashlib

base = bytearray((i * 7 + i // 251) % 256 for i in range(75_000))
filler = bytes((i * 13 + 5) % 256 for i in range(5_000))
bad = 0
for n in [0, 1, 2, 31, 32, 33, 63, 64, 65, 100, 4095, 4096, 4097, 10_000, 70_000]:
    h = hashlib.sha256()
    for gap in [1, 2, 31, 32, 33, 100, 4095, 4096, 4097, 5000]:
        for at in sorted({0, 1, n // 2, max(n - 33, 0), max(n - 1, 0), n}):
            if at > n:
                continue
            b = bytearray(base[:n + gap])
            del b[n:]
            b[at:at] = filler[:gap]
            want = bytes(base[:at]) + filler[:gap] + bytes(base[at:n])
            bad += b != want
            h.update(bytes(b))
            # one byte, by insert, and a slice that grows
            c = bytearray(base[:n + 1])
            c.pop()
            c.insert(at, gap % 256)
            bad += c != bytes(base[:at]) + bytes([gap % 256]) + bytes(base[at:n])
            h.update(bytes(c))
            if n - at >= 2:
                d = bytearray(base[:n + gap])
                del d[n:]
                d[at:at + 2] = filler[:gap + 2]
                bad += d != bytes(base[:at]) + filler[:gap + 2] + bytes(base[at + 2:n])
                h.update(bytes(d))
    print(n, h.hexdigest()[:24])
print('bad', bad)
