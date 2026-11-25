# List equality resumes at the next index after a user comparison, using
# both lists' current lengths rather than the lengths at entry.


class Effect:
    def __init__(self, action, equal=True):
        self.action = action
        self.equal = equal

    def __eq__(self, other):
        self.action()
        return self.equal


for side in (0, 1):
    for action in ('shrink', 'clear', 'grow'):
        a, b = [None, 1, 2], [0, 1, 2]
        victim = (a, b)[side]
        change = {'shrink': victim.pop, 'clear': victim.clear, 'grow': lambda: victim.append(3)}
        a[0] = Effect(change[action])
        assert not (a == b), (side, action)

# Temporary length inequality does not short-circuit the remaining pairs.
a, b = [None, None], [0, 0]
a[0] = Effect(lambda: a.append(1))
a[1] = Effect(lambda: b.append(1))
assert a == b

# If both lists grow, even pairs beyond the original end are compared.
a, b = [None], [0]


def grow_both():
    a.append(1)
    b.append(2)


a[0] = Effect(grow_both)
assert a != b

# The live lengths matter at each nesting level, including when an inner
# comparison mutates an outer list. Both lists can shorten and still equal.
a, b = [None, 1], [0, 1]
a[0] = Effect(lambda: (a.pop(), b.pop()))
assert [a] == [b]

a, b = [[None], 1], [[0], 1]
a[0][0] = Effect(b.clear)
assert not (a == b)

# If the pair itself is gone, CPython compares the lengths even when
# its comparison said the items differed.
a, b = [None, 1], [0, 2]
a[0] = Effect(lambda: (a.clear(), b.clear()), False)
assert a == b

# A false comparison still decides the result if its index survives, even
# if the new items at that index would compare equal.
a, b = [None], [0]

def replace_item():
    a[0] = 0

a[0] = Effect(replace_item, False)
assert a != b
