for op in ('-=', '^=', 'difference_update', 'symmetric_difference_update'):
    s = set(range(30))
    alias = s
    if op == '-=':
        s -= s
    elif op == '^=':
        s ^= s
    elif op == 'difference_update':
        assert s.difference_update(s) is None
    else:
        assert s.symmetric_difference_update(s) is None
    assert s is alias and not s
    # Inserting into the cleared table differs from inserting into a
    # larger table left behind by repeated deletes.
    s.update([40, 9, 33, 8, 16, 1])
    assert list(s) == [33, 1, 40, 9, 8, 16], op


class Key:
    def __hash__(self):
        return 0

    def __eq__(self, other):
        calls.append('eq')
        return False


a, b = Key(), Key()
calls = []
s = {a, b}
alias = s
calls.clear()
s |= s
assert s is alias and not calls
assert list(s)[0] is a and list(s)[1] is b
assert s.update(s) is None and not calls
