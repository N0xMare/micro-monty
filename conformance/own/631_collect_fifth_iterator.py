class One:
    def __eq__(self, other):
        return other == 1


items = {'k': One()}.items()
a, b, c, d, pairs = (iter([1]), iter([2]), iter([3]), iter([4]), iter([('k', 1)]))
target = []
whole = slice(None)
# No calls or loop edges between these collections. The fifth iterator
# must still be whole when the intersection hands off for One.__eq__.
target[whole] = a
target[whole] = b
target[whole] = c
target[whole] = d
assert items & pairs == {('k', 1)}
