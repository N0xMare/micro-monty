# random, from CPython 3.14's Lib/random.py (PSF license, see THIRD_PARTY.md).
# The generator (CPython's C `_random.Random`) is MT19937 in mm-engine's
# random.rs; seeding without a seed draws from the host's entropy source
# (docs/spec.md §10), as CPython draws from os.urandom.
from math import log as _log, exp as _exp, pi as _pi, e as _e, ceil as _ceil
from math import sqrt as _sqrt, acos as _acos, cos as _cos, sin as _sin
from math import tau as TWOPI, floor as _floor, isfinite as _isfinite
from math import lgamma as _lgamma, fabs as _fabs, log2 as _log2
from operator import index as _index
from itertools import accumulate as _accumulate, repeat as _repeat
from bisect import bisect as _bisect
from _mm import clock as _clock, urandom as _urandom
__all__ = ['Random', 'SystemRandom', 'betavariate', 'binomialvariate', 'choice', 'choices', 'expovariate', 'gammavariate', 'gauss', 'getrandbits', 'getstate', 'lognormvariate', 'normalvariate', 'paretovariate', 'randbytes', 'randint', 'random', 'randrange', 'sample', 'seed', 'setstate', 'shuffle', 'triangular', 'uniform', 'vonmisesvariate', 'weibullvariate']
NV_MAGICCONST = 4 * _exp(-0.5) / _sqrt(2.0)
LOG4 = _log(4.0)
SG_MAGICCONST = 1.0 + _log(4.5)
BPF = 53
RECIP_BPF = 2 ** (-BPF)
_ONE = 1
_sha512 = None
_SEQUENCES = (list, tuple, str, range, bytes)


def _is_sequence(x):
    # collections.abc.Sequence, for the types micro-monty has.
    if isinstance(x, _SEQUENCES):
        return True
    if isinstance(x, (dict, set)):
        return False
    t = type(x)
    return hasattr(t, '__getitem__') and hasattr(t, '__len__') and hasattr(t, 'index') and hasattr(t, 'count')


def _u64(k):
    k = _index(k)
    if k < 0:
        raise ValueError('Cannot convert negative int')
    if k >= 18446744073709551616:
        raise OverflowError('Python int too large for C uint64_t')
    return k


class _BaseRandom:
    # CPython's `_random.Random`.

    def __init__(self, x=None):
        self.seed(x)

    def random(self):
        return _mt_random(self._mt)

    def seed(self, n=None):
        if n is None:
            try:
                key = _urandom(2496)
            except RuntimeError:
                # No entropy from the host: the clock, else a fixed seed.
                try:
                    n = int(_clock() * 1000000)
                except RuntimeError:
                    n = 0
                key = n.to_bytes((n.bit_length() + 7) // 8, 'little')
        else:
            if isinstance(n, int):
                n = abs(int(n))
            else:
                n = hash(n)
                if n < 0:
                    n += 18446744073709551616
            key = n.to_bytes((n.bit_length() + 7) // 8, 'little')
        if '_mt' not in self.__dict__:
            self._mt = _mt_new()
        _mt_seed(self._mt, key)

    def getstate(self):
        return _mt_getstate(self._mt)

    def setstate(self, state):
        if not isinstance(state, tuple):
            raise TypeError('state vector must be a tuple')
        if len(state) != 625:
            raise ValueError('state vector is the wrong size')
        words = []
        for i in range(624):
            x = state[i]
            if not isinstance(x, int):
                raise TypeError('an integer is required')
            if x < 0:
                raise OverflowError("can't convert negative value to unsigned int")
            if x >= 18446744073709551616:
                raise OverflowError('Python int too large to convert to C unsigned long')
            words.append(x & 4294967295)
        index = _index(state[624])
        if index < 0 or index > 624:
            raise ValueError('invalid state')
        words.append(index)
        if '_mt' not in self.__dict__:
            self._mt = _mt_new()
        _mt_setstate(self._mt, tuple(words))

    def getrandbits(self, *args, **kwargs):
        if kwargs:
            raise TypeError('Random.getrandbits() takes no keyword arguments')
        if len(args) != 1:
            raise TypeError(f'getrandbits() takes exactly one argument ({len(args)} given)')
        k = _u64(args[0])
        r = _mt_bits(self._mt, k)
        if type(r) is bytes:
            return int.from_bytes(r, 'little')
        return r


class Random(_BaseRandom):
    VERSION = 3

    def __init__(self, x=None):
        self.seed(x)
        self.gauss_next = None

    def seed(self, a=None, version=2):
        if version == 1 and isinstance(a, (str, bytes)):
            a = a.decode('latin-1') if isinstance(a, bytes) else a
            x = ord(a[0]) << 7 if a else 0
            for c in map(ord, a):
                x = (1000003 * x ^ c) & 18446744073709551615
            x ^= len(a)
            a = -2 if x == -1 else x
        elif version == 2 and isinstance(a, (str, bytes)):
            global _sha512
            if _sha512 is None:
                from hashlib import sha512 as _sha512
            if isinstance(a, str):
                a = a.encode()
            a = int.from_bytes(a + _sha512(a).digest(), 'big')
        elif not isinstance(a, (type(None), int, float, str, bytes)):
            raise TypeError('The only supported seed types are:\nNone, int, float, str, bytes, and bytearray.')
        super().seed(a)
        self.gauss_next = None

    def getstate(self):
        return (self.VERSION, super().getstate(), self.gauss_next)

    def setstate(self, state):
        version = state[0]
        if version == 3:
            version, internalstate, self.gauss_next = state
            super().setstate(internalstate)
        elif version == 2:
            version, internalstate, self.gauss_next = state
            try:
                internalstate = tuple((_mod32(x) for x in internalstate))
            except ValueError as e:
                raise TypeError from e
            super().setstate(internalstate)
        else:
            raise ValueError(f'state with version {version} passed to Random.setstate() of version {self.VERSION}')

    def __getstate__(self):
        return self.getstate()

    def __setstate__(self, state):
        self.setstate(state)

    def __reduce__(self):
        return self.__class__, (), self.getstate()


    def _randbelow(self, n):
        cls = type(self)
        if cls is not Random:
            # A subclass overriding random() but not getrandbits() draws
            # from random() (CPython's __init_subclass__ dispatch).
            for c in cls.__mro__:
                if '_randbelow' in c.__dict__ or c is Random:
                    break
                if 'getrandbits' in c.__dict__:
                    break
                if 'random' in c.__dict__:
                    return self._randbelow_without_getrandbits(n)
        if n < 18446744073709551616 and type(self).getrandbits is _BaseRandom.getrandbits:
            return _mt_below(self._mt, n)
        return self._randbelow_with_getrandbits(n)

    def _randbelow_with_getrandbits(self, n):
        k = n.bit_length()
        r = self.getrandbits(k)
        while r >= n:
            r = self.getrandbits(k)
        return r

    def _randbelow_without_getrandbits(self, n, maxsize=1 << BPF):
        random = self.random
        if n >= maxsize:
            return _floor(random() * n)
        rem = maxsize % n
        limit = (maxsize - rem) / maxsize
        r = random()
        while r >= limit:
            r = random()
        return _floor(r * maxsize) % n

    def randbytes(self, n):
        return self.getrandbits(n * 8).to_bytes(n, 'little')

    def randrange(self, start, stop=None, step=_ONE):
        istart = _index(start)
        if stop is None:
            if step is not _ONE:
                raise TypeError('Missing a non-None stop argument')
            if istart > 0:
                return self._randbelow(istart)
            raise ValueError('empty range for randrange()')
        istop = _index(stop)
        width = istop - istart
        istep = _index(step)
        if istep == 1:
            if width > 0:
                return istart + self._randbelow(width)
            raise ValueError(f'empty range in randrange({start}, {stop})')
        if istep > 0:
            n = (width + istep - 1) // istep
        elif istep < 0:
            n = (width + istep + 1) // istep
        else:
            raise ValueError('zero step for randrange()')
        if n <= 0:
            raise ValueError(f'empty range in randrange({start}, {stop}, {step})')
        return istart + istep * self._randbelow(n)

    def randint(self, a, b):
        a = _index(a)
        b = _index(b)
        if b < a:
            raise ValueError(f'empty range in randint({a}, {b})')
        return a + self._randbelow(b - a + 1)

    def choice(self, seq):
        if not len(seq):
            raise IndexError('Cannot choose from an empty sequence')
        return seq[self._randbelow(len(seq))]

    def shuffle(self, x):
        randbelow = self._randbelow
        for i in reversed(range(1, len(x))):
            j = randbelow(i + 1)
            x[i], x[j] = (x[j], x[i])

    def sample(self, population, k, *, counts=None):
        if not _is_sequence(population):
            raise TypeError('Population must be a sequence.  For dicts or sets, use sorted(d).')
        n = len(population)
        if counts is not None:
            cum_counts = list(_accumulate(counts))
            if len(cum_counts) != n:
                raise ValueError('The number of counts does not match the population')
            total = cum_counts.pop() if cum_counts else 0
            if not isinstance(total, int):
                raise TypeError('Counts must be integers')
            if total < 0:
                raise ValueError('Counts must be non-negative')
            selections = self.sample(range(total), k=k)
            bisect = _bisect
            return [population[bisect(cum_counts, s)] for s in selections]
        randbelow = self._randbelow
        if not 0 <= k <= n:
            raise ValueError('Sample larger than population or is negative')
        result = [None] * k
        setsize = 21
        if k > 5:
            setsize += 4 ** _ceil(_log(k * 3, 4))
        if n <= setsize:
            pool = list(population)
            for i in range(k):
                j = randbelow(n - i)
                result[i] = pool[j]
                pool[j] = pool[n - i - 1]
        else:
            selected = set()
            selected_add = selected.add
            for i in range(k):
                j = randbelow(n)
                while j in selected:
                    j = randbelow(n)
                selected_add(j)
                result[i] = population[j]
        return result

    def choices(self, population, weights=None, *, cum_weights=None, k=1):
        random = self.random
        n = len(population)
        if cum_weights is None:
            if weights is None:
                floor = _floor
                n += 0.0
                return [population[floor(random() * n)] for i in _repeat(None, k)]
            try:
                cum_weights = list(_accumulate(weights))
            except TypeError:
                if not isinstance(weights, int):
                    raise
                k = weights
                raise TypeError(f'The number of choices must be a keyword argument: k={k!r}') from None
        elif weights is not None:
            raise TypeError('Cannot specify both weights and cumulative weights')
        if len(cum_weights) != n:
            raise ValueError('The number of weights does not match the population')
        total = cum_weights[-1] + 0.0
        if total <= 0.0:
            raise ValueError('Total of weights must be greater than zero')
        if not _isfinite(total):
            raise ValueError('Total of weights must be finite')
        bisect = _bisect
        hi = n - 1
        return [population[bisect(cum_weights, random() * total, 0, hi)] for i in _repeat(None, k)]

    def uniform(self, a, b):
        return a + (b - a) * self.random()

    def triangular(self, low=0.0, high=1.0, mode=None):
        u = self.random()
        try:
            c = 0.5 if mode is None else (mode - low) / (high - low)
        except ZeroDivisionError:
            return low
        if u > c:
            u = 1.0 - u
            c = 1.0 - c
            low, high = (high, low)
        return low + (high - low) * _sqrt(u * c)

    def normalvariate(self, mu=0.0, sigma=1.0):
        random = self.random
        while True:
            u1 = random()
            u2 = 1.0 - random()
            z = NV_MAGICCONST * (u1 - 0.5) / u2
            zz = z * z / 4.0
            if zz <= -_log(u2):
                break
        return mu + z * sigma

    def gauss(self, mu=0.0, sigma=1.0):
        random = self.random
        z = self.gauss_next
        self.gauss_next = None
        if z is None:
            x2pi = random() * TWOPI
            g2rad = _sqrt(-2.0 * _log(1.0 - random()))
            z = _cos(x2pi) * g2rad
            self.gauss_next = _sin(x2pi) * g2rad
        return mu + z * sigma

    def lognormvariate(self, mu, sigma):
        return _exp(self.normalvariate(mu, sigma))

    def expovariate(self, lambd=1.0):
        return -_log(1.0 - self.random()) / lambd

    def vonmisesvariate(self, mu, kappa):
        random = self.random
        if kappa <= 1e-06:
            return TWOPI * random()
        s = 0.5 / kappa
        r = s + _sqrt(1.0 + s * s)
        while True:
            u1 = random()
            z = _cos(_pi * u1)
            d = z / (r + z)
            u2 = random()
            if u2 < 1.0 - d * d or u2 <= (1.0 - d) * _exp(d):
                break
        q = 1.0 / r
        f = (q + z) / (1.0 + q * z)
        u3 = random()
        if u3 > 0.5:
            theta = (mu + _acos(f)) % TWOPI
        else:
            theta = (mu - _acos(f)) % TWOPI
        return theta

    def gammavariate(self, alpha, beta):
        if alpha <= 0.0 or beta <= 0.0:
            raise ValueError('gammavariate: alpha and beta must be > 0.0')
        random = self.random
        if alpha > 1.0:
            ainv = _sqrt(2.0 * alpha - 1.0)
            bbb = alpha - LOG4
            ccc = alpha + ainv
            while True:
                u1 = random()
                if not 1e-07 < u1 < 0.9999999:
                    continue
                u2 = 1.0 - random()
                v = _log(u1 / (1.0 - u1)) / ainv
                x = alpha * _exp(v)
                z = u1 * u1 * u2
                r = bbb + ccc * v - x
                if r + SG_MAGICCONST - 4.5 * z >= 0.0 or r >= _log(z):
                    return x * beta
        elif alpha == 1.0:
            return -_log(1.0 - random()) * beta
        else:
            while True:
                u = random()
                b = (_e + alpha) / _e
                p = b * u
                if p <= 1.0:
                    x = p ** (1.0 / alpha)
                else:
                    x = -_log((b - p) / alpha)
                u1 = random()
                if p > 1.0:
                    if u1 <= x ** (alpha - 1.0):
                        break
                elif u1 <= _exp(-x):
                    break
            return x * beta

    def betavariate(self, alpha, beta):
        y = self.gammavariate(alpha, 1.0)
        if y:
            return y / (y + self.gammavariate(beta, 1.0))
        return 0.0

    def paretovariate(self, alpha):
        u = 1.0 - self.random()
        return u ** (-1.0 / alpha)

    def weibullvariate(self, alpha, beta):
        u = 1.0 - self.random()
        return alpha * (-_log(u)) ** (1.0 / beta)

    def binomialvariate(self, n=1, p=0.5):
        if n < 0:
            raise ValueError('n must be non-negative')
        if p <= 0.0 or p >= 1.0:
            if p == 0.0:
                return 0
            if p == 1.0:
                return n
            raise ValueError('p must be in the range 0.0 <= p <= 1.0')
        random = self.random
        if n == 1:
            return _index(random() < p)
        if p > 0.5:
            return n - self.binomialvariate(n, 1.0 - p)
        if n * p < 10.0:
            x = y = 0
            c = _log2(1.0 - p)
            if not c:
                return x
            while True:
                try:
                    y += _floor(_log2(random()) / c) + 1
                except ValueError:
                    continue
                if y > n:
                    return x
                x += 1
        assert n * p >= 10.0 and p <= 0.5
        setup_complete = False
        spq = _sqrt(n * p * (1.0 - p))
        b = 1.15 + 2.53 * spq
        a = -0.0873 + 0.0248 * b + 0.01 * p
        c = n * p + 0.5
        vr = 0.92 - 4.2 / b
        while True:
            u = random()
            u -= 0.5
            us = 0.5 - _fabs(u)
            try:
                k = _floor((2.0 * a / us + b) * u + c)
            except ZeroDivisionError:
                continue
            if k < 0 or k > n:
                continue
            v = random()
            if us >= 0.07 and v <= vr:
                return k
            if not setup_complete:
                alpha = (2.83 + 5.1 / b) * spq
                lpq = _log(p / (1.0 - p))
                m = _floor((n + 1) * p)
                h = _lgamma(m + 1) + _lgamma(n - m + 1)
                setup_complete = True
            v *= alpha / (a / (us * us) + b)
            if _log(v) <= h - _lgamma(k + 1) - _lgamma(n - k + 1) + (k - m) * lpq:
                return k

class SystemRandom(Random):

    def random(self):
        return (int.from_bytes(_urandom(7), 'big') >> 3) * RECIP_BPF

    def getrandbits(self, k):
        if k < 0:
            raise ValueError('number of bits must be non-negative')
        numbytes = (k + 7) // 8
        x = int.from_bytes(_urandom(numbytes), 'big')
        return x >> numbytes * 8 - k

    def randbytes(self, n):
        return _urandom(n)

    def seed(self, *args, **kwds):
        if '_mt' not in self.__dict__:
            self._mt = _mt_new()
        return None

    def _notimplemented(self, *args, **kwds):
        raise NotImplementedError('System entropy source does not have state.')
    getstate = setstate = _notimplemented
_inst = Random()
seed = _inst.seed
random = _inst.random
uniform = _inst.uniform
triangular = _inst.triangular
randint = _inst.randint
choice = _inst.choice
randrange = _inst.randrange
sample = _inst.sample
shuffle = _inst.shuffle
choices = _inst.choices
normalvariate = _inst.normalvariate
lognormvariate = _inst.lognormvariate
expovariate = _inst.expovariate
vonmisesvariate = _inst.vonmisesvariate
gammavariate = _inst.gammavariate
gauss = _inst.gauss
betavariate = _inst.betavariate
binomialvariate = _inst.binomialvariate
paretovariate = _inst.paretovariate
weibullvariate = _inst.weibullvariate
getstate = _inst.getstate
setstate = _inst.setstate
getrandbits = _inst.getrandbits
randbytes = _inst.randbytes



def _mod32(x):
    # `x % 2 ** 32` with CPython's errors for operands `%` rejects.
    if isinstance(x, str):
        raise TypeError('not all arguments converted during string formatting')
    if isinstance(x, float):
        raise TypeError('an integer is required')
    return x % 4294967296
