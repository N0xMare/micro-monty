# __getattr__ fallbacks, __setattr__/__delattr__, and their interplay.
class Proxy:
    def __init__(self, target):
        object.__setattr__(self, '_target', target)
        object.__setattr__(self, 'log', [])

    def __getattr__(self, name):
        self.log.append(name)
        if name.startswith('secret'):
            raise AttributeError(f'no secrets: {name}')
        return getattr(self._target, name)

    def __setattr__(self, name, value):
        self.log.append('set ' + name)
        setattr(self._target, name, value)

    def __delattr__(self, name):
        self.log.append('del ' + name)
        delattr(self._target, name)


class Target:
    def __init__(self):
        self.x = 1

    def hello(self):
        return 'hello from target'


t = Target()
p = Proxy(t)
print(p.x, p.hello(), p.log)
p.y = 5
print(t.y, 'y' in p.__dict__, p.log)
del p.y
print(hasattr(t, 'y'), p.log)
try:
    p.secret_key
except AttributeError as e:
    print('AttributeError:', e)
print(hasattr(p, 'secret2'), getattr(p, 'secret3', 'default'), hasattr(p, 'x'))
try:
    p.nothing
except AttributeError as e:
    print('AttributeError:', e)


class Lazy:
    def __init__(self):
        self.computed = []

    def __getattr__(self, name):
        if name.startswith('_'):
            raise AttributeError(name)
        value = len(name)
        setattr(self, name, value)
        self.computed.append(name)
        return value


lz = Lazy()
print(lz.abc, lz.abc, lz.hello, lz.computed)


class Prop:
    @property
    def broken(self):
        raise AttributeError('inner')

    def __getattr__(self, name):
        return f'fallback:{name}'


print(Prop().broken, Prop().other)


class Frozen:
    def __init__(self, v):
        object.__setattr__(self, 'v', v)

    def __setattr__(self, name, value):
        raise AttributeError(f'{type(self).__name__} is frozen')

    def __delattr__(self, name):
        raise AttributeError('cannot delete')


f = Frozen(3)
print(f.v)
for action in (lambda: setattr(f, 'v', 4), lambda: delattr(f, 'v')):
    try:
        action()
    except AttributeError as e:
        print(e)
try:
    f.w = 1
except AttributeError as e:
    print(e)


class Counted:
    def __setattr__(self, name, value):
        super().__setattr__(name, value * 2)


c = Counted()
c.a = 4
c.a += 1
print(c.a, vars(c))


class GetattrRecursion:
    def __getattr__(self, name):
        return self.missing_too


try:
    GetattrRecursion().x
except RecursionError as e:
    print('RecursionError', str(e)[:30])
print(getattr(Target(), 'x'), getattr(Target, 'hello')(t))
try:
    getattr(t, 'nope')
except AttributeError as e:
    print(e)
