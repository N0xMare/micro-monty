calls = []


class Key:
    def __init__(self, name):
        self.name = name

    def __hash__(self):
        calls.append(('hash', self.name))
        return 0

    def __eq__(self, other):
        calls.append((self.name, other.name))
        return False


keys = [Key(i) for i in range(4)]
s = set(keys)
t = s.copy()
earlier = [(0, 1), (0, 2), (1, 2), (0, 3), (1, 3), (2, 3)]
calls.clear()
assert not (s - s)
assert calls == earlier
calls.clear()
assert not s.difference(t)
assert calls == earlier
calls.clear()
assert s <= t
assert calls == earlier
assert [x.name for x in s] == [0, 1, 2, 3]
assert [x.name for x in t] == [0, 1, 2, 3]

# Remove in reverse probe order so all the earlier colliders are still live.
t = set(reversed(keys))
calls.clear()
assert s.difference_update(t) is None
assert calls == [(0, 3), (1, 3), (2, 3), (0, 2), (1, 2), (0, 1)]
assert not s
assert [x.name for x in t] == [3, 2, 1, 0]
