# IntEnum/IntFlag members (int subclasses without their own __index__)
# work as ints where builtins take one: slice bounds, hex/bin/oct, bytes(n),
# str.find/count start and end, sequence.index bounds.
from enum import IntEnum, IntFlag
class N(IntEnum):
    TWO = 2
    THREE = 3
class F(IntFlag):
    R = 4
    W = 2
xs = [10, 20, 30, 40]
tests = [lambda: xs[N.TWO], lambda: xs[N.TWO:N.THREE], lambda: list(range(N.THREE)), lambda: 'ab' * N.TWO, lambda: [0] * N.TWO,
 lambda: round(3.14159, N.TWO), lambda: (1, 2, 3)[N.TWO], lambda: 'abcd'[N.TWO], lambda: bytes(N.TWO), lambda: chr(65 + N.TWO),
 lambda: N.TWO + 1, lambda: N.THREE // 2, lambda: divmod(N.THREE, N.TWO), lambda: hex(N.THREE), lambda: bin(F.R | F.W),
 lambda: f'{N.TWO:03d}', lambda: '%d|%x|%s' % (N.TWO, N.THREE, N.TWO), lambda: format(N.THREE, 'b'), lambda: int(N.TWO),
 lambda: float(N.TWO), lambda: abs(-N.TWO), lambda: pow(N.TWO, N.THREE), lambda: pow(2, 3, N.THREE), lambda: xs.pop(N.TWO),
 lambda: xs.insert(N.TWO, 0) or xs, lambda: 'abcdef'.ljust(N.THREE), lambda: 'a,b,c'.split(',', N.TWO[0] if False else N.TWO) ,
 lambda: sum([N.TWO, N.THREE]), lambda: max(N), lambda: sorted([N.THREE, 1]), lambda: {N.TWO: 'x'}[2], lambda: N.TWO == 2,
 lambda: isinstance(N.TWO, int), lambda: N.TWO.bit_length(), lambda: (N.THREE).to_bytes(2, 'big'), lambda: str(F.R | F.W),
 lambda: F.R & 4, lambda: math.comb(N.THREE, N.TWO), lambda: list(itertools.islice('abcdef', N.TWO, None)),
 lambda: 'x'.zfill(N.THREE), lambda: '{:>{}}'.format('a', N.THREE), lambda: bytearray(b'abc')[N.TWO], lambda: memo.get(N.TWO),
 lambda: [1,2,3,4][::N.TWO], lambda: list(enumerate('ab', N.TWO)), lambda: 'abc'.find('c', N.TWO), lambda: int('11', N.TWO), lambda: N.TWO < 2.5,
]
import math, itertools
memo = {2: 'two'}
for i, t in enumerate(tests):
    try: r = t()
    except Exception as e: r = f'{type(e).__name__}: {e}'
    print(i, repr(r))
print([1, 2, 3, 4].index(3, N.TWO), (5, 6, 7).index(7, 0, N.THREE), 'abcabc'.count('a', N.TWO), oct(N.THREE))
