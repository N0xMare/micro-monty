class Sep(str):
    pass


for split in ('a b'.split, 'a b'.rsplit):
    assert split(None) == ['a', 'b']
    assert split(Sep(' ')) == ['a', 'b']
    for sep, name in ((1, 'int'), (b' ', 'bytes')):
        try:
            split(sep=sep)
        except TypeError as e:
            assert str(e) == 'must be str or None, not ' + name
        else:
            raise AssertionError('non-string separator accepted')
