# Generator expressions are lazy: evaluation order and side effects.
log = []


def src(n):
    for i in range(n):
        log.append(f'src{i}')
        yield i


def f(x):
    log.append(f'f{x}')
    return x * 2


g = (f(x) for x in src(3))
print(log)
print(next(g), log)
print(list(g), log)
log.clear()

g = (x for x in src(2) if log.append(f'test{x}') is None)
print(log)
print(list(g), log)
log.clear()


def first_iter():
    log.append('outer iterable built')
    return range(2)


g = (x + y for x in first_iter() for y in src(2))
print(log)
print(list(g))
print(log)
log.clear()

g = (1 / x for x in [1, 0, 2])
print(next(g))
try:
    next(g)
except ZeroDivisionError as e:
    print('lazy error', e)
print(next(g, 'gen closed after error'))

try:
    g = (x for x in 5)
except TypeError as e:
    print('eager iter()', e)

gen = (i * i for i in range(5))
print(type(gen).__name__, gen.__name__, sum(gen), sum(gen))
print(any(x > 2 for x in src(10)), log)
log.clear()
print(all(x < 1 for x in src(10)), log)
log.clear()
print(','.join(str(x) for x in range(4)), max(x % 3 for x in range(10)), min((x for x in []), default='empty'))
print(sorted(x for x in 'hello'), tuple(x for x in 'ab'), set(x % 2 for x in range(5)))
print(dict((x, x * x) for x in range(3)), list(zip((x for x in 'ab'), range(5))))
print(list(enumerate(x for x in 'xy')), list(map(str, (x for x in range(3)))))
print(list(filter(None, (x % 3 for x in range(7)))))
n = 10
g = (n * x for x in range(2))
n = 100
print(list(g))
it = iter([1, 2, 3, 4])
print(list(x for x in it if x % 2), list(it))
sq = (x * x for x in range(3))
print(3 in (x for x in range(5)), 4 in sq, list(sq))
print(list(iter(lambda: log.append(1) or len(log), 3)), log)
