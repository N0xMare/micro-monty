class Key:
    def __hash__(self):
        return 7


def items(target):
    print('seen by __init__ source', len(target))
    yield 5


s = {Key()}
set.__init__(s, s)
print('self', len(s))
s.add(Key())
set.__init__(s, items(s))
print('replacement', sorted(s))

s = {1, 2}
try:
    set.__init__(s, 3)
except TypeError as e:
    print(type(e).__name__, str(e), sorted(s))


class WatchedKey:
    def __hash__(self):
        print('hashed')
        return 19


d = {WatchedKey(): 2}
set.__init__(s, d)
set.__init__(s, d.keys())
print(len(s))
