# with-statements inside generators; closing a generator runs __exit__.
class Tracker:
    def __init__(self, name):
        self.name = name

    def __enter__(self):
        print('enter', self.name)
        return self

    def __exit__(self, t, v, tb):
        print('exit', self.name, t.__name__ if t else None)
        return t is KeyError


def producer():
    with Tracker('producer'):
        yield 1
        yield 2
    print('producer after with')


g = producer()
print(next(g))
g.close()
print('---')
g = producer()
print(next(g))
try:
    g.throw(KeyError('suppressed'))
except StopIteration:
    print('stopped after suppression')
print('---')
print(list(producer()))
print('---')


def manual_cm():
    print('setup')
    try:
        yield 'resource'
    finally:
        print('teardown')


class FromGen:
    def __init__(self, gen):
        self.gen = gen

    def __enter__(self):
        return next(self.gen)

    def __exit__(self, t, v, tb):
        if t is None:
            next(self.gen, None)
        else:
            try:
                self.gen.throw(v)
            except type(v):
                return False
        return False


with FromGen(manual_cm()) as r:
    print('using', r)
try:
    with FromGen(manual_cm()) as r:
        raise ValueError('inside')
except ValueError as e:
    print('propagated', e)


def gen_in_with():
    with Tracker('outer'):
        for x in range(3):
            with Tracker(f'inner{x}'):
                if x == 1:
                    return 'early'
                yield x


g = gen_in_with()
print(list(g))
