calls = []


class Key:
    def __init__(self, name):
        self.name = name

    def __hash__(self):
        calls.append(('hash', self.name))
        return 0

    def __eq__(self, other):
        global perturb
        calls.append((self.name, other.name))
        if self.name == 'b' and other.name == 'new':
            if perturb == 'delete':
                active.discard(first)
            elif perturb == 'fill':
                perturb = None
                active.add(Key('fill'))
        return False


first = Key('a')
for mode in ('delete', None, 'fill'):
    perturb = mode
    active = {first, Key('b'), Key('c')}
    if mode != 'delete':
        active.discard(first)
    calls.clear()
    active.add(Key('new'))
    if mode == 'delete':
        assert [x.name for x in active] == ['b', 'c', 'new']
        assert calls == [('hash', 'new'), ('a', 'new'), ('b', 'new'), ('hash', 'a'), ('c', 'new')]
    elif mode is None:
        assert [x.name for x in active] == ['new', 'b', 'c']
        assert calls == [('hash', 'new'), ('b', 'new'), ('c', 'new')]
    else:
        # Filling the remembered dummy forces the original probe to restart.
        assert [x.name for x in active] == ['fill', 'b', 'c', 'new']
        assert calls == [
            ('hash', 'new'), ('b', 'new'),
            ('hash', 'fill'), ('b', 'fill'), ('c', 'fill'), ('c', 'new'),
            ('fill', 'new'), ('b', 'new'), ('c', 'new'),
        ]
