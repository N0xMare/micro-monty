class EqualsZero:
    def __eq__(self, other):
        return other == 0


# The matched item must be tested, and the iterator still has its tail.
it = iter([0, 1])
print(EqualsZero() in it, list(it))
it = iter([1, EqualsZero(), 2])
print(0 not in it, list(it))
