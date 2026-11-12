# Uncaught errors from __exit__, __enter__ and the with body.
class Resource:
    def __init__(self, name):
        self.name = name

    def __enter__(self):
        print('open', self.name)
        return self

    def __exit__(self, exc_type, exc, tb):
        print('close', self.name, exc_type.__name__ if exc_type else None)
        if exc_type is KeyError:
            raise RuntimeError(f'cleanup of {self.name} failed')
        return False


def work():
    with Resource('db') as r, Resource('cache'):
        data = {}
        return data[r.name]


work()
