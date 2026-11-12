# match statements: patterns the corpus covers thinly.
from collections import deque, OrderedDict, defaultdict, UserDict, UserList


class Point:
    __match_args__ = ('x', 'y')

    def __init__(self, x, y):
        self.x = x
        self.y = y


class Eq:
    def __init__(self):
        self.calls = []

    def __eq__(self, other):
        self.calls.append(other)
        return other == 3


class Seq:
    def __init__(self, items):
        self.items = items

    def __len__(self):
        print('len called')
        return len(self.items)

    def __getitem__(self, i):
        return self.items[i]


def describe(x):
    match x:
        case None | True | False as s:
            return ('singleton', s)
        case 0 | 0.0:
            return 'zero'
        case -1:
            return 'minus one'
        case 1 + 2j:
            return 'complex'
        case 'a' 'b':
            return 'ab'
        case b'raw':
            return 'bytes'
        case (1, 2) | [3, 4]:
            return 'pair'
        case [x, y, *rest] if rest:
            return ('long', x, y, rest)
        case [*head, last]:
            return ('head-last', head, last)
        case {'k': {'n': [n, *_]}}:
            return ('nested', n)
        case {'k': v, **kw} if not kw:
            return ('only k', v)
        case {**kw}:
            return ('mapping', sorted(kw))
        case Point(x=0) | Point(y=0):
            return 'on an axis'
        case Point(a, b) as p if a == b:
            return ('diagonal', a, p.x)
        case Point():
            return 'point'
        case str() | bytes():
            return 'text'
        case _:
            return 'other'


for v in [None, True, False, 0, 0.0, -1, 1 + 2j, 'ab', b'raw', (1, 2), [3, 4], [1, 2, 3],
          (5,), [7, 8], {'k': {'n': [9, 10]}}, {'k': 1}, {'k': 1, 'z': 2}, {}, Point(0, 5),
          Point(2, 2), Point(1, 2), 'text', 3.5, range(3), deque([1, 2, 3]), 'xy', OrderedDict(k=4),
          defaultdict(int, {'k': 5}), UserDict({'k': 6}), UserList([1, 2, 3])]:
    print(type(v).__name__ if isinstance(v, Point) else repr(v)[:30], describe(v))

# a value pattern compares subject == value with the subject on the left
e = Eq()
match e:
    case 3:
        print('matched 3')
    case _:
        print('no 3')
print(e.calls)

# sequences: str, bytes and dict views are not sequences; user classes are
# not unless registered; iterators are not
for subject in ['ab', b'ab', {1: 2}.keys(), iter([1, 2]), Seq([1, 2])]:
    match subject:
        case [a, b]:
            print('sequence', a, b)
        case _:
            print('not a sequence', type(subject).__name__)

# star wildcard indexes; named star copies into a list
match (1, 2, 3, 4):
    case (first, *_, last):
        print(first, last)
match range(5):
    case [0, *mid, 4]:
        print(mid)

# capture names bind only when the whole pattern matches, before the guard
x = 'unchanged'
match [1, 2]:
    case [x, 3]:
        pass
    case _:
        pass
print(x)
match [5, 6]:
    case [x, y] if x > y:
        pass
    case _:
        print('guard failed, x bound:', x, y)

# class patterns: builtins match themselves positionally
for v in [7, 7.5, 'seven', [7], (7,), {7: 7}, {7}, b'7', True]:
    match v:
        case bool(b):
            print('bool', b)
        case int(i):
            print('int', i)
        case float(f) | str(f):
            print('float or str', f)
        case list(l) | tuple(l):
            print('list or tuple', l)
        case dict(d) | set(d) | bytes(d):
            print('other builtin', d)


# class pattern errors
class NoArgs:
    pass


class BadArgs:
    __match_args__ = ['x']


class Dup:
    __match_args__ = ('x', 'x')
    x = 1


for cls, subject in [(NoArgs, NoArgs()), (BadArgs, BadArgs()), (Dup, Dup()), (len, 1), (int, 1)]:
    try:
        match subject:
            case cls(a):
                print('matched', a)
            case _:
                print('no match')
    except TypeError as exc:
        print('TypeError:', exc)
try:
    match Dup():
        case Dup(z, x=1):
            pass
except TypeError as exc:
    print('TypeError:', exc)

# missing attributes just fail the match
match Point(1, 2):
    case Point(z=3):
        print('impossible')
    case Point(x=1, y=y):
        print('y is', y)

# mapping patterns use get(), so __missing__ is not called
dd = defaultdict(list)
match dd:
    case {'a': _}:
        print('found a')
    case _:
        print('no a; keys:', list(dd))

# **rest copies the remaining items
match {'a': 1, 'b': 2, 'c': 3}:
    case {'b': b, **rest}:
        print(b, rest)

# match subject tuples and walrus
match 1, 2:
    case (a, b):
        print('tuple subject', a, b)
match (n := 10):
    case 10:
        print('walrus subject', n)


# match in a function with nonlocal and global captures
g = 0


def outer():
    cell = None

    def inner(v):
        nonlocal cell
        global g
        match v:
            case [cell, g]:
                return True
        return False

    print(inner([1, 2]), cell, g)


outer()


# match in a generator and in a loop with break/continue
def gen(items):
    for item in items:
        match item:
            case 'stop':
                break
            case 'skip':
                continue
            case _:
                yield item


print(list(gen(['a', 'skip', 'b', 'stop', 'c'])))

# A unary plus begins a match subject, but `match` is still a soft keyword.
match +3:
    case 3:
        print('positive subject')
match = 4
print(match + 3)
