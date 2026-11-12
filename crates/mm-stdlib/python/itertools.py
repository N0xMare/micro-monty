# itertools: the iterator classes of CPython 3.14's itertools module (C in
# CPython), written after the pure-Python equivalents in its documentation
# (PSF license, see THIRD_PARTY.md). Every iterator is lazy and pausable;
# argument errors use CPython's messages.

_MAXSIZE = 9223372036854775807
_MISSING = object()


def _type_repr(t):
    if t is ...:
        return '...'
    if isinstance(t, type):
        if t.__module__ == 'builtins':
            return t.__qualname__
        return t.__module__ + '.' + t.__qualname__
    return repr(t)


class _GenericAlias:
    # A minimal `types.GenericAlias` for `chain[...]`.
    def __init__(self, origin, args):
        self.__origin__ = origin
        self.__args__ = args if isinstance(args, tuple) else (args,)

    def __call__(self, *args, **kwargs):
        return self.__origin__(*args, **kwargs)

    def __repr__(self):
        if not self.__args__:
            inner = '()'
        else:
            inner = ', '.join([_type_repr(a) for a in self.__args__])
        return _type_repr(self.__origin__) + '[' + inner + ']'


def _class_getitem(cls, item):
    return _GenericAlias(cls, item)


def _parse(name, args, kwargs, params, required):
    # PyArg_ParseTupleAndKeywords: positional-or-keyword `params`, the first
    # `required` of them required.
    n = len(args) + len(kwargs)
    if n > len(params):
        raise TypeError(f'{name}() takes at most {len(params)} argument'
                        f'{"" if len(params) == 1 else "s"} ({n} given)')
    values = list(args) + [_MISSING] * (len(params) - len(args))
    for i in range(len(args), len(params)):
        if params[i] in kwargs:
            values[i] = kwargs[params[i]]
        elif i < required:
            raise TypeError(f"{name}() missing required argument '{params[i]}' (pos {i + 1})")
    for k in kwargs:
        if k not in params:
            raise TypeError(f"{name}() got an unexpected keyword argument '{k}'")
        i = params.index(k)
        if i < len(args):
            raise TypeError(f"argument for {name}() given by name ('{k}') and position ({i + 1})")
    return values


def _no_kwargs(name, kwargs):
    if kwargs:
        raise TypeError(f'{name}() takes no keyword arguments')


def _unpack(name, args, lo, hi):
    # PyArg_UnpackTuple's messages.
    n = len(args)
    if n < lo:
        if lo == hi:
            raise TypeError(f'{name} expected {lo} argument{"" if lo == 1 else "s"}, got {n}')
        raise TypeError(f'{name} expected at least {lo} argument{"" if lo == 1 else "s"}, got {n}')
    if n > hi:
        if lo == hi:
            raise TypeError(f'{name} expected {hi} argument{"" if hi == 1 else "s"}, got {n}')
        raise TypeError(f'{name} expected at most {hi} argument{"" if hi == 1 else "s"}, got {n}')


def _ssize(x):
    if isinstance(x, int):
        n = int(x)
    elif hasattr(type(x), '__index__'):
        n = x.__index__()
    else:
        raise TypeError(f"'{type(x).__name__}' object cannot be interpreted as an integer")
    if n > _MAXSIZE or n < -_MAXSIZE - 1:
        raise OverflowError('Python int too large to convert to C ssize_t')
    return n


def _is_number(x):
    return isinstance(x, (int, float, complex)) or hasattr(type(x), '__index__') or hasattr(type(x), '__float__')


class count:
    def __init__(self, *args, **kwargs):
        start, step = _parse('count', args, kwargs, ('start', 'step'), 0)
        if start is _MISSING:
            start = 0
        if step is _MISSING:
            step = 1
        if not _is_number(start) or not _is_number(step):
            raise TypeError('a number is required')
        if type(start) is bool:
            start = int(start)
        self._step_given = not (type(step) is int and step == 1 or step is True)
        if type(step) is bool:
            step = int(step)
        self._n = start
        self._step = step

    def __iter__(self):
        return self

    def __next__(self):
        n = self._n
        self._n = n + self._step
        return n

    def __repr__(self):
        if self._step_given:
            return f'count({self._n!r}, {self._step!r})'
        return f'count({self._n!r})'



class cycle:
    def __init__(self, *args, **kwargs):
        _no_kwargs('cycle', kwargs)
        _unpack('cycle', args, 1, 1)
        self._it = iter(args[0])
        self._saved = []
        self._index = 0
        self._first = True

    def __iter__(self):
        return self

    def __next__(self):
        if self._it is not None:
            for x in self._it:
                if self._first:
                    self._saved.append(x)
                return x
            self._it = None
        if not self._saved:
            raise StopIteration
        x = self._saved[self._index]
        self._index = (self._index + 1) % len(self._saved)
        return x


class repeat:
    def __init__(self, *args, **kwargs):
        obj, times = _parse('repeat', args, kwargs, ('object', 'times'), 1)
        self._obj = obj
        if times is _MISSING:
            self._times = None
        else:
            times = _ssize(times)
            self._times = times if times > 0 else 0

    def __iter__(self):
        return self

    def __next__(self):
        if self._times is None:
            return self._obj
        if self._times <= 0:
            raise StopIteration
        self._times -= 1
        return self._obj

    def __length_hint__(self):
        if self._times is None:
            raise TypeError('len() of unsized object')
        return self._times

    def __repr__(self):
        if self._times is None:
            return f'repeat({self._obj!r})'
        return f'repeat({self._obj!r}, {self._times})'


class accumulate:
    def __init__(self, *args, **kwargs):
        if len(args) > 2 and len(args) + len(kwargs) <= 3:
            raise TypeError(f'accumulate() takes at most 2 positional arguments ({len(args)} given)')
        iterable, func, initial = _parse('accumulate', args, kwargs, ('iterable', 'func', 'initial'), 1)
        self._it = iter(iterable)
        self._func = None if func is _MISSING else func
        self._initial = None if initial is _MISSING else initial
        self._total = _MISSING

    def __iter__(self):
        return self

    def __next__(self):
        if self._initial is not None:
            self._total = self._initial
            self._initial = None
            return self._total
        x = next(self._it)
        if self._total is _MISSING:
            self._total = x
        elif self._func is None:
            self._total = self._total + x
        else:
            self._total = self._func(self._total, x)
        return self._total


class batched:
    def __init__(self, *args, **kwargs):
        if len(args) > 2 and len(args) + len(kwargs) <= 3:
            raise TypeError(f'batched() takes exactly 2 positional arguments ({len(args)} given)')
        iterable, n, strict = _parse('batched', args, kwargs, ('iterable', 'n', 'strict'), 2)
        n = _ssize(n)
        if n < 1:
            raise ValueError('n must be at least one')
        self._it = iter(iterable)
        self._n = n
        self._strict = False if strict is _MISSING else bool(strict)

    def __iter__(self):
        return self

    def __next__(self):
        if self._it is None:
            raise StopIteration
        batch = []
        for x in self._it:
            batch.append(x)
            if len(batch) == self._n:
                break
        if not batch:
            self._it = None
            raise StopIteration
        if self._strict and len(batch) != self._n:
            self._it = None
            raise ValueError('batched(): incomplete batch')
        return tuple(batch)


class chain:
    def __init__(self, *iterables, **kwargs):
        _no_kwargs('chain', kwargs)
        self._source = iter(iterables)
        self._active = None

    @classmethod
    def from_iterable(cls, *args, **kwargs):
        _no_kwargs('chain.from_iterable', kwargs)
        if len(args) != 1:
            raise TypeError(f'chain.from_iterable() takes exactly one argument ({len(args)} given)')
        c = cls()
        c._source = iter(args[0])
        return c

    def __iter__(self):
        return self

    def __next__(self):
        while True:
            if self._active is None:
                if self._source is None:
                    raise StopIteration
                try:
                    it = next(self._source)
                    self._active = iter(it)
                except BaseException:
                    self._source = None
                    raise
            active = self._active
            if active is None:
                continue
            for x in active:
                return x
            if self._active is active:
                self._active = None

    __class_getitem__ = classmethod(_class_getitem)


class compress:
    def __init__(self, *args, **kwargs):
        data, selectors = _parse('compress', args, kwargs, ('data', 'selectors'), 2)
        self._data = iter(data)
        self._selectors = iter(selectors)

    def __iter__(self):
        return self

    def __next__(self):
        while True:
            d = next(self._data)
            s = next(self._selectors)
            if s:
                return d


class dropwhile:
    def __init__(self, *args, **kwargs):
        _no_kwargs('dropwhile', kwargs)
        _unpack('dropwhile', args, 2, 2)
        self._pred = args[0]
        self._it = iter(args[1])
        self._dropping = True

    def __iter__(self):
        return self

    def __next__(self):
        while True:
            x = next(self._it)
            if not self._dropping:
                return x
            if not self._pred(x):
                self._dropping = False
                return x


class takewhile:
    def __init__(self, *args, **kwargs):
        _no_kwargs('takewhile', kwargs)
        _unpack('takewhile', args, 2, 2)
        self._pred = args[0]
        self._it = iter(args[1])
        self._done = False

    def __iter__(self):
        return self

    def __next__(self):
        if self._done:
            raise StopIteration
        x = next(self._it)
        if self._pred(x):
            return x
        self._done = True
        raise StopIteration


class filterfalse:
    def __init__(self, *args, **kwargs):
        _no_kwargs('filterfalse', kwargs)
        _unpack('filterfalse', args, 2, 2)
        self._pred = args[0]
        self._it = iter(args[1])

    def __iter__(self):
        return self

    def __next__(self):
        while True:
            x = next(self._it)
            if self._pred is None:
                if not x:
                    return x
            elif not self._pred(x):
                return x


class starmap:
    def __init__(self, *args, **kwargs):
        _no_kwargs('starmap', kwargs)
        _unpack('starmap', args, 2, 2)
        self._func = args[0]
        self._it = iter(args[1])

    def __iter__(self):
        return self

    def __next__(self):
        args = next(self._it)
        if not isinstance(args, tuple):
            args = tuple(args)
        return self._func(*args)


class islice:
    def __init__(self, *args, **kwargs):
        _no_kwargs('islice', kwargs)
        _unpack('islice', args, 2, 4)
        self._it = iter(args[0])
        start, stop, step = 0, None, 1
        if len(args) == 2:
            stop = args[1]
            if stop is not None:
                stop = self._bound(stop, 'Stop argument for islice() must be None or an integer: 0 <= x <= sys.maxsize.')
        else:
            if args[1] is not None:
                start = self._bound(args[1], 'Indices for islice() must be None or an integer: 0 <= x <= sys.maxsize.')
            if args[2] is not None:
                stop = self._bound(args[2], 'Stop argument for islice() must be None or an integer: 0 <= x <= sys.maxsize.')
            if len(args) == 4 and args[3] is not None:
                step = self._bound(args[3], 'Step for islice() must be a positive integer or None.')
                if step == 0:
                    raise ValueError('Step for islice() must be a positive integer or None.')
        self._next = start
        self._stop = stop
        self._step = step
        self._pos = 0

    @staticmethod
    def _bound(x, msg):
        if isinstance(x, int) or hasattr(type(x), '__index__'):
            # An int's value as PyNumber_Index reads it: enumerate's start
            # is read natively, where `int(x)` would call a subclass's
            # `__int__`.
            n = x.__index__() if not isinstance(x, int) else next(enumerate((0,), x))[0]
            if n < 0 or n > _MAXSIZE:
                raise ValueError(msg)
            return n
        raise ValueError(msg)

    def __iter__(self):
        return self

    def __next__(self):
        it = self._it
        if it is None:
            raise StopIteration
        while self._pos < self._next:
            if self._stop is not None and self._pos >= self._stop:
                self._it = None
                raise StopIteration
            try:
                next(it)
            except StopIteration:
                self._it = None
                raise
            self._pos += 1
        if self._stop is not None and self._pos >= self._stop:
            self._it = None
            raise StopIteration
        try:
            x = next(it)
        except StopIteration:
            self._it = None
            raise
        self._pos += 1
        self._next += self._step
        if self._stop is not None and self._next > self._stop:
            self._next = self._stop
        return x


class pairwise:
    def __init__(self, *args, **kwargs):
        _no_kwargs('pairwise', kwargs)
        _unpack('pairwise', args, 1, 1)
        self._it = iter(args[0])
        self._old = _MISSING

    def __iter__(self):
        return self

    def __next__(self):
        # As CPython's `pairwise_next`: `old` is held across the pull, so a
        # nested step of this iterator does not change the pair it makes.
        it = self._it
        if it is None:
            raise StopIteration
        old = self._old
        if old is _MISSING:
            try:
                old = next(it)
            except StopIteration:
                self._old = _MISSING
                self._it = None
                raise
            self._old = old
            it = self._it
            if it is None:
                self._old = _MISSING
                raise StopIteration
        try:
            new = next(it)
        except StopIteration:
            self._it = None
            self._old = _MISSING
            raise
        self._old = new
        return (old, new)


class zip_longest:
    def __init__(self, *args, **kwargs):
        fill = None
        for k in kwargs:
            if k != 'fillvalue':
                raise TypeError(f"zip_longest() got an unexpected keyword argument '{k}'")
            fill = kwargs[k]
        self._its = [iter(a) for a in args]
        self._active = len(self._its)
        self._fill = fill

    def __iter__(self):
        return self

    def __next__(self):
        if not self._its or self._active == 0:
            raise StopIteration
        result = []
        for i, it in enumerate(self._its):
            if it is None:
                result.append(self._fill)
                continue
            for x in it:
                result.append(x)
                break
            else:
                self._active -= 1
                if self._active == 0:
                    raise StopIteration
                self._its[i] = None
                result.append(self._fill)
        return tuple(result)


class groupby:
    # Mirrors CPython's C groupby step by step, so re-entrant key functions
    # see the same states.
    def __init__(self, *args, **kwargs):
        iterable, key = _parse('groupby', args, kwargs, ('iterable', 'key'), 1)
        self._it = iter(iterable)
        self._keyfunc = None if key is _MISSING else key
        self._tgtkey = _MISSING
        self._currkey = _MISSING
        self._currvalue = _MISSING
        self._currgrouper = None

    def __iter__(self):
        return self

    def _step(self):
        v = next(self._it)
        k = v if self._keyfunc is None else self._keyfunc(v)
        self._currvalue = v
        self._currkey = k

    def __next__(self):
        self._currgrouper = None
        while True:
            if self._currkey is _MISSING:
                pass
            elif self._tgtkey is _MISSING:
                break
            elif not (self._tgtkey is self._currkey or self._tgtkey == self._currkey):
                break
            self._step()
        self._tgtkey = self._currkey
        g = _grouper(self, self._tgtkey)
        self._currgrouper = g
        return (self._currkey, g)


class _grouper:
    def __init__(self, parent, tgtkey):
        self._parent = parent
        self._tgtkey = tgtkey

    def __iter__(self):
        return self

    def __next__(self):
        p = self._parent
        if p._currgrouper is not self:
            raise StopIteration
        if p._currvalue is _MISSING:
            p._step()
        if not (self._tgtkey is p._currkey or self._tgtkey == p._currkey):
            raise StopIteration
        v = p._currvalue
        p._currvalue = _MISSING
        return v


class _tee_data:
    def __init__(self, it):
        self.it = iter(it)
        self.values = []
        self.base = 0
        self.running = False


class _tee:
    def __init__(self, data, index):
        self._data = data
        self._index = index

    def __iter__(self):
        return self

    def __next__(self):
        d = self._data
        i = self._index
        if i >= d.base + len(d.values):
            if d.running:
                raise RuntimeError('cannot re-enter the tee iterator')
            d.running = True
            try:
                x = next(d.it)
            finally:
                d.running = False
            d.values.append(x)
        x = d.values[i - d.base]
        self._index = i + 1
        return x

    def __copy__(self):
        return _tee(self._data, self._index)


def _tee_py(args):
    # The body of the native `tee` (a builtin function, as in CPython),
    # which checked the number of arguments.
    n = _ssize(args[1]) if len(args) > 1 else 2
    if n < 0:
        raise ValueError('n must be >= 0')
    if n == 0:
        return ()
    if n > _MAXSIZE // 8:
        raise MemoryError
    it = iter(args[0])
    if isinstance(it, _tee):
        first = it.__copy__()
    else:
        first = _tee(_tee_data(it), 0)
    result = [first]
    for _ in range(n - 1):
        result.append(first.__copy__())
    return tuple(result)


def _pool(name, iterable):
    return tuple(iterable)


class product:
    def __init__(self, *args, **kwargs):
        repeat = 1
        for k in kwargs:
            if k != 'repeat':
                raise TypeError(f"product() got an unexpected keyword argument '{k}'")
            repeat = _ssize(kwargs[k])
        if repeat < 0:
            raise ValueError('repeat argument cannot be negative')
        if repeat and len(args) > _MAXSIZE // 8 // repeat:
            raise OverflowError('repeat argument too large')
        pools = [tuple(pool) for pool in args] if repeat else []
        self._pools = pools * repeat
        self._indices = None
        self._done = False

    def __iter__(self):
        return self

    def __next__(self):
        if self._done:
            raise StopIteration
        pools = self._pools
        if self._indices is None:
            for p in pools:
                if not p:
                    self._done = True
                    raise StopIteration
            self._indices = [0] * len(pools)
            return tuple(p[0] for p in pools)
        indices = self._indices
        i = len(pools) - 1
        while i >= 0:
            indices[i] += 1
            if indices[i] < len(pools[i]):
                return tuple(pools[k][indices[k]] for k in range(len(pools)))
            indices[i] = 0
            i -= 1
        self._done = True
        raise StopIteration


def _r_arg(r):
    r = _ssize(r)
    if r < 0:
        raise ValueError('r must be non-negative')
    return r


class permutations:
    def __init__(self, *args, **kwargs):
        iterable, r = _parse('permutations', args, kwargs, ('iterable', 'r'), 1)
        pool = tuple(iterable)
        n = len(pool)
        if r is _MISSING or r is None:
            r = n
        elif not isinstance(r, int):
            raise TypeError('Expected int as r')
        else:
            r = _r_arg(r)
        self._pool = pool
        self._r = r
        self._indices = list(range(n))
        self._cycles = list(range(n, n - r, -1))
        self._started = False
        self._done = r > n

    def __iter__(self):
        return self

    def __next__(self):
        if self._done:
            raise StopIteration
        pool, r, indices, cycles = self._pool, self._r, self._indices, self._cycles
        n = len(pool)
        if not self._started:
            self._started = True
            return tuple(pool[i] for i in indices[:r])
        if n:
            for i in reversed(range(r)):
                cycles[i] -= 1
                if cycles[i] == 0:
                    indices[i:] = indices[i + 1:] + indices[i:i + 1]
                    cycles[i] = n - i
                else:
                    j = cycles[i]
                    indices[i], indices[-j] = indices[-j], indices[i]
                    return tuple(pool[i] for i in indices[:r])
        self._done = True
        raise StopIteration


class combinations:
    def __init__(self, *args, **kwargs):
        iterable, r = _parse('combinations', args, kwargs, ('iterable', 'r'), 2)
        r = _ssize(r)
        self._pool = tuple(iterable)
        r = _r_arg(r)
        self._r = r
        self._indices = list(range(r))
        self._started = False
        self._done = r > len(self._pool)

    def __iter__(self):
        return self

    def __next__(self):
        if self._done:
            raise StopIteration
        pool, r, indices = self._pool, self._r, self._indices
        n = len(pool)
        if not self._started:
            self._started = True
            return tuple(pool[i] for i in indices)
        for i in reversed(range(r)):
            if indices[i] != i + n - r:
                break
        else:
            self._done = True
            raise StopIteration
        indices[i] += 1
        for j in range(i + 1, r):
            indices[j] = indices[j - 1] + 1
        return tuple(pool[i] for i in indices)


class combinations_with_replacement:
    def __init__(self, *args, **kwargs):
        iterable, r = _parse('combinations_with_replacement', args, kwargs, ('iterable', 'r'), 2)
        r = _ssize(r)
        self._pool = tuple(iterable)
        r = _r_arg(r)
        self._r = r
        self._indices = [0] * r
        self._started = False
        self._done = not self._pool and r > 0

    def __iter__(self):
        return self

    def __next__(self):
        if self._done:
            raise StopIteration
        pool, r, indices = self._pool, self._r, self._indices
        n = len(pool)
        if not self._started:
            self._started = True
            return tuple(pool[i] for i in indices)
        for i in reversed(range(r)):
            if indices[i] != n - 1:
                break
        else:
            self._done = True
            raise StopIteration
        indices[i:] = [indices[i] + 1] * (r - i)
        return tuple(pool[i] for i in indices)


for _c in (count, cycle, repeat, accumulate, batched, chain, compress, dropwhile,
           takewhile, filterfalse, starmap, islice, pairwise, zip_longest, groupby,
           _grouper, _tee, product, permutations, combinations,
           combinations_with_replacement):
    _c.__mm_tpname__ = 'itertools.' + _c.__name__
del _c
