# `int.from_bytes` reads its byte order before its bytes, as CPython's
# argument parsing does, and names itself in the byte order's TypeError;
# an iterable's items are read as `bytes()` reads them.
def show(f):
    try:
        print(repr(f()))
    except Exception as e:
        print(type(e).__name__ + ':', e)


for src in [b'\x01\xff', [1, 255], [256], [-1], [1.5], 'ab', 5, [], b'\xff' * 70]:
    for order in ('big', 'little', 'middle', 5):
        show(lambda: int.from_bytes(src, order, signed=True))
show(lambda: int.from_bytes([1, 2], byteorder=None))
show(lambda: int.from_bytes(bytearray(b'\x80\x00'), 'little', signed=1))
show(lambda: (5).to_bytes(2, 5))
show(lambda: (5).to_bytes(2, 'mid'))
show(lambda: (5).to_bytes(2, None))

# __bytes__, int subclasses and bool, and the clinic messages (review L1).
class B:
    def __bytes__(self):
        return b'\x01\x00'


class Bad:
    def __bytes__(self):
        return 'x'


class I(int):
    pass


class J(int):
    def __new__(cls, v):
        return super().__new__(cls, v + 1)


def show(f):
    try:
        r = f()
    except Exception as e:
        r = (type(e).__name__, str(e))
    print(repr(r), type(r).__name__)


show(lambda: int.from_bytes(B()))
show(lambda: int.from_bytes(B(), 'little'))
show(lambda: I.from_bytes(B(), signed=True))
show(lambda: int.from_bytes(Bad()))
show(lambda: bool.from_bytes(b'\x01'))
show(lambda: bool.from_bytes(b'\x00\x00'))
show(lambda: bool.from_bytes([0, 2], 'little'))
show(lambda: I.from_bytes(b'\x01'))
show(lambda: J.from_bytes(b'\x01'))
show(lambda: I.from_bytes(b'\xff', signed=True))
show(lambda: I(5).from_bytes(b'\x02'))
show(lambda: I.from_bytes(b'\x01', 'middle'))
show(lambda: int.from_bytes(b'\x01', 'big', True))
show(lambda: (1).to_bytes(1, 'big', True))
show(lambda: int.from_bytes())
show(lambda: int.from_bytes(byteorder='big'))
show(lambda: bool.from_bytes())
show(lambda: int.from_bytes(b'', 'big', signed=True, x=1))
show(lambda: float.fromhex())
show(lambda: bytes.fromhex())
show(lambda: 'a'.split('a', 1, 2))
show(lambda: int.from_bytes(b'\x01\x00' * 20000).bit_length())

# A bytearray's block is read in place, up to its length (review T2).
ba = bytearray(b'\x80\x01\x02')
ba.extend(b'\xff' * 100)
del ba[3:]
show(lambda: int.from_bytes(ba))
show(lambda: int.from_bytes(ba, 'little', signed=True))
show(lambda: I.from_bytes(ba, signed=True))
show(lambda: int.from_bytes(bytearray()))
show(lambda: int.from_bytes(bytearray(b'\x00' * 70_000 + b'\x07')))
show(lambda: int.from_bytes(bytearray(b'\xff' * 70_000), 'little', signed=True))
