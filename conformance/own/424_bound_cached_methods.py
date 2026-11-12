# lru_cache, cache and partial bound to an instance are real bound methods,
# as in CPython: the wrapper's attributes (cache_info, cache_clear,
# __wrapped__, __name__) reach through `obj.method`, and repr shows the method.
import functools


class Prices:
    def __init__(self, rate):
        self.rate = rate
        self.calls = 0

    @functools.lru_cache(maxsize=32)
    def convert(self, amount):
        """Convert one amount."""
        self.calls += 1
        return round(amount * self.rate, 2)

    @functools.cache
    def label(self, code):
        return f'{code}:{self.rate}'

    shout = functools.partial(lambda prefix, self, text: f'{prefix}{self.rate}{text.upper()}', '>')


p = Prices(1.5)
print([p.convert(10), p.convert(10), p.convert(4)], p.calls)
print(p.convert.cache_info())
print(Prices.convert.cache_info() == p.convert.cache_info())
p.convert.cache_clear()
print(p.convert.cache_info(), p.convert(10), p.calls)
print(p.label('EUR'), p.label('EUR'), p.label.cache_info().hits)
print(p.convert.__name__, p.convert.__qualname__, p.convert.__doc__)
print(p.convert.__wrapped__.__name__, p.convert.__self__ is p)
print(type(p.convert).__name__, type(p.shout).__name__, p.shout('hi'))
print(repr(p.convert).startswith('<bound method Prices.convert of <__main__.Prices object'))
print(repr(p.shout).startswith('<bound method ? of <__main__.Prices object'))
print(p.convert == p.convert, p.convert is p.convert, hash(p.convert) == hash(p.convert))
print(p.convert.cache_parameters())
q = Prices(2.0)
print(q.convert(10), p.convert.cache_info().currsize)
try:
    p.convert.cache_info(1)
except TypeError as e:
    print('TypeError', e)

# Bound methods hash by receiver and function, so the same method made
# twice is one dict key or set member (handler registries).
class Handler:
    def on_event(self, e):
        return e


h, g = Handler(), Handler()
registry = {h.on_event: 'h', g.on_event: 'g'}
print(registry.get(h.on_event), registry.get(g.on_event), len(registry))
print(h.on_event in {h.on_event}, hash(h.on_event) == hash(h.on_event), h.on_event == g.on_event)
seen = set()
for _ in range(3):
    seen.add(h.on_event)
    seen.add(p.convert)
print(len(seen))
xs = [1]
print(hash(xs.append) == hash(xs.append), {xs.append: 1}.get(xs.append), xs.append == xs.append)
