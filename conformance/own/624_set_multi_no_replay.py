armed = False


class Collides:
    def __hash__(self):
        return 0

    def __eq__(self, other):
        if armed:
            raise AssertionError('the first source was merged again')
        return False


keys = {Collides(), Collides()}
armed = True
for n in (65_535, 65_536, 65_537):
    s = set()
    s.update(keys, 'a' * n)
    assert len(s) == 3

# A generator runs only after the earlier prefix is merged, and sees its
# own items inserted before it resumes. Later native sources still run.
def items():
    assert s == {1, 2}
    yield 3
    assert s == {1, 2, 3}


s = set()
it = iter([1, 2])
s.update(it, s, items(), it, {4: None}, [5])
assert s == {1, 2, 3, 4, 5}
assert list(it) == []
