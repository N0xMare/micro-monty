s = {1, 2, 3}


class BadHash:
    def __hash__(self):
        print('hash sees', sorted(s))
        raise ValueError('hash')


try:
    s.intersection_update([1, 2], [BadHash()])
except ValueError as e:
    print(type(e).__name__, str(e), sorted(s))


def items():
    print('iterator sees', sorted(s))
    yield 2
    yield 3


s.intersection_update([1, 2], items())
print(sorted(s))
