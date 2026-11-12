import struct
x = struct.unpack('%d?' % (M // 10), bytes(M // 10))
