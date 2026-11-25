armed = False


class Key:
    def __hash__(self):
        if armed:
            raise ValueError('rehash')
        return 41


key = Key()
d = {key: None}
armed = True

# Update fast-merges an exact dict using stored hashes. The first
# argument of difference also uses them when probing the larger dict.
s = set()
s.update([1], d)
assert any(x is key for x in s)
assert s.difference(d, []) == {1}

# A later difference argument (and any difference_update argument) goes
# through the dict's iterator, so the key's hash is called again.
for method, first in (('difference', []), ('difference_update', [1]),
                      ('intersection_update', [1])):
    try:
        getattr(s, method)(first, d)
    except ValueError as error:
        assert str(error) == 'rehash'
    else:
        raise AssertionError(method)
assert list(s) == [key]
