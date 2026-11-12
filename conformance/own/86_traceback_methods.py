# Tracebacks through methods, properties, dunders and generators.
class Account:
    def __init__(self, balance):
        self.balance = balance

    @property
    def ratio(self):
        return 100 / self.balance

    def withdraw(self, n):
        return self._check(n)

    def _check(self, n):
        if n > self.balance:
            raise ValueError(f'insufficient funds: {n} > {self.balance}')
        return self.balance - n

    def __add__(self, other):
        return Account(self.balance + other.balance)

    def __len__(self):
        return self.balance

    def __iter__(self):
        yield self.balance
        yield self.missing


def report(e):
    print(type(e).__name__, e)


for action in [lambda: Account(0).ratio, lambda: Account(1).withdraw(5), lambda: Account(1) + 3,
               lambda: list(Account(2)), lambda: len(Account(-1))]:
    try:
        action()
    except Exception as e:
        report(e)


class Handler:
    def handle(self, items):
        return [self.transform(x) for x in items]

    def transform(self, x):
        return {'a': 1}[x]


def main():
    acct = Account(10)
    print(acct.withdraw(3))
    Handler().handle(['a', 'b'])


main()
