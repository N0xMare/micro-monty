# random: seeded streams match CPython's Mersenne Twister bit for bit.
import random

for seed in (0, 1, 42, 2**70, -5, 'abc', b'xyz', 3.5):
    random.seed(seed)
    print(repr(seed), random.random(), random.randint(1, 10**6), random.getrandbits(70))
r = random.Random(12345)
print(r.randrange(10), r.randrange(5, 100, 7), r.choice('abcdef'), r.choices(range(10), k=5), r.choices('ab', weights=[1, 9], k=6))
xs = list(range(20))
r.shuffle(xs)
print(xs, r.sample(range(100), 6), r.sample(['a', 'b', 'c'], k=3, counts=[1, 2, 3])[:3])
print(r.uniform(1, 2), r.triangular(0, 10, 3), r.gauss(0, 1), r.normalvariate(5, 2), r.lognormvariate(0, 1))
print(r.expovariate(2), r.betavariate(2, 3), r.gammavariate(2, 1), r.paretovariate(3), r.weibullvariate(1, 2), r.vonmisesvariate(1, 2))
print(r.binomialvariate(20, 0.3), r.randbytes(5), r.getrandbits(1), r.random())
s = r.getstate()
a = [r.random() for _ in range(3)]
r.setstate(s)
print(a == [r.random() for _ in range(3)])
random.seed(7, version=1)
print(random.random())
for bad in [lambda: r.randrange(0), lambda: r.choice([]), lambda: r.sample([1], 2), lambda: r.randint(3, 1), lambda: r.seed([1])]:
    try:
        bad()
    except (ValueError, IndexError, TypeError) as e:
        print(type(e).__name__, e)
