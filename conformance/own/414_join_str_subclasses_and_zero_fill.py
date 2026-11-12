# str.join takes str subclasses (StrEnum members, a str subclass as the
# separator), which raised "expected str instance"; a format spec's `0`
# flag pads with zeros also after an explicit alignment (`<08` is
# `12000000`), and `=` stays an error for strings.
from enum import StrEnum
class Color(StrEnum):
    RED = 'red'
    BLUE = 'blue'
print(', '.join([Color.RED, Color.BLUE]), '-'.join(Color), '/'.join(c for c in Color))
class S(str): pass
print(repr(''.join([S('abcd')])), type(''.join([S('a')])).__name__, S('x').join(['a', S('b')]),
      type(S('x').join(['a', 'b'])).__name__)
try:
    ', '.join([Color.RED, 3])
except TypeError as e:
    print(e)

for spec in ['08s', '<08s', '>08s', '^08s', '<08', '>08', '^08', '=08', 'x<08', '*>08',
             '<08.2f', '>+08,d', '^#08x', '<08b', '08', '+08', '_<010_']:
    row = []
    for v in ['ab', 12, -3.5, 255]:
        try:
            row.append(format(v, spec))
        except (ValueError, TypeError) as e:
            row.append('E:' + str(e))
    print(spec, row)
print('%08s|%-08d|%08.3f' % ('ab', 5, 2.5), f'{7:<05}', f'{-7:<05}', '{:<05}'.format(True))
