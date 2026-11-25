armed = False


class Key:
    def __init__(self, n):
        self.n = n

    def __hash__(self):
        return 0

    def __eq__(self, other):
        global armed
        if armed and self is a2 and other is b:
            armed = False
            for x in list(t):
                if x is not b2 and x is not c2:
                    t.discard(x)
        return self.n == other.n


for op in ('-=', 'difference_update', '&=', 'intersection_update', '&', 'intersection'):
    a, b, c = Key(1), Key(2), Key(3)
    a2, b2, c2 = Key(1), Key(2), Key(3)
    s = {a, b, c}
    t = {a2, b2, c2}
    t.update(range(100, 140))
    alias = s
    armed = True
    if op == '-=':
        s -= t
    elif op == 'difference_update':
        assert s.difference_update(t) is None
    elif op == '&=':
        s &= t
    elif op == 'intersection_update':
        assert s.intersection_update(t) is None
    elif op == '&':
        s = s & t
    else:
        s = s.intersection(t)
    assert sorted(x.n for x in t) == [2, 3]
    assert (s is alias) == (op not in ('&', 'intersection'))
    # The comparison shrank t to two keys after a matched. A set source
    # must still run through c, even when the output already holds a and b.
    expected = [] if op in ('-=', 'difference_update') else [1, 2, 3]
    assert sorted(x.n for x in s) == expected, op

# A non-set source still stops after the last match, even if inserting
# that match grew the result's table.
source = iter([0, 1, 2, 3, 4, 99])
assert set(range(5)).intersection(source) == set(range(5))
assert next(source) == 99
