d = {1: 'x'}
for view, source in ((d.keys(), [0, 1, 2]), (d.items(), [(0, 'a'), (1, 'x'), (2, 'b')])):
    it = iter(source)
    assert not view.isdisjoint(it)
    assert list(it) == source[2:]
