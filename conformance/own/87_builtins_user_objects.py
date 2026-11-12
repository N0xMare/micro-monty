# Builtins that reach user code: sum, in, index/count/remove, dict/set methods.
class Money:
    def __init__(self, cents):
        self.cents = cents

    def __add__(self, o):
        if isinstance(o, int):
            return Money(self.cents + o)
        return Money(self.cents + o.cents)

    __radd__ = __add__

    def __eq__(self, o):
        return isinstance(o, Money) and self.cents == o.cents

    def __hash__(self):
        return hash(self.cents)

    def __lt__(self, o):
        return self.cents < o.cents

    def __repr__(self):
        return f'${self.cents / 100:.2f}'


wallet = [Money(150), Money(275), Money(5)]
print(sum(wallet), sum(wallet, Money(1000)), sum([], Money(0)))
print(Money(5) in wallet, Money(6) in wallet, wallet.index(Money(275)), wallet.count(Money(5)))
print(min(wallet), max(wallet), sorted(wallet), max(wallet, key=lambda m: -m.cents))
print(min([], default=Money(0)), max(wallet, default=None), min(Money(3), Money(2), Money(9)))
wallet.remove(Money(150))
print(wallet, (Money(5),) == (Money(5),), [Money(1)] < [Money(2)])
try:
    wallet.index(Money(1))
except ValueError as e:
    print(e)
d = {}
d.setdefault(Money(1), []).append('a')
d.setdefault(Money(1), []).append('b')
d.update({Money(2): 'two'})
d.update([(Money(3), 'three')])
print(d, d.get(Money(2)), d.pop(Money(3)), d.pop(Money(9), 'missing'), Money(1) in d.keys())
print(list(d.items()), dict.fromkeys([1]) if False else dict(zip([Money(7)], [7])))
s = set(wallet)
s.update([Money(5), Money(6)])
print(sorted(s), Money(6) in s, s.issubset({Money(5), Money(275), Money(6), Money(0)}))
s.remove(Money(6))
print(len(s), sorted(s | {Money(1)}), sorted(s & {Money(5)}), sorted(s - {Money(5)}))
print(sorted(s ^ {Money(5), Money(9)}), s == {Money(5), Money(275)}, s <= s, s < {Money(5)})
print(any(Money(0) == m for m in wallet), all(m.cents > 1 for m in wallet))
print(list(zip(wallet, 'ab')), list(enumerate(wallet, 1)), list(reversed(wallet)))
print(tuple(map(lambda m: m.cents, wallet)), list(filter(lambda m: m.cents > 10, wallet)))
print(', '.join(map(repr, wallet)), str(wallet), f'{wallet}')
print({m: m.cents for m in wallet}, {m for m in wallet if m.cents})
print(sorted({Money(3): 'c', Money(1): 'a'}.items()), len({Money(1), Money(1), Money(2)}))
try:
    sum([Money(1), 'x'])
except TypeError as e:
    print(e)
try:
    s.remove(Money(404))
except KeyError as e:
    print('KeyError', e)
try:
    d[Money(404)]
except KeyError as e:
    print('KeyError', e)
print(list(zip([1, 2], (x for x in 'ab'), strict=True)))
try:
    list(zip([1, 2], (x for x in 'a'), strict=True))
except ValueError as e:
    print(e)
