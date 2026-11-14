# The prelude: Python code that builtins and VM operations hand off to when
# they meet user objects or generators (docs/spec.md §5). A native fast path
# handles builtin-typed data; on the first value that needs Python code
# (a user `__eq__`, `__hash__`, `__repr__`, a `key=` function, a generator)
# the operation continues here, in the same dispatch loop, so every
# safepoint can still pause.
#
# Twins of builtins are named `_<name>` (functions) or `_<type>_<name>`
# (methods); they get the positional arguments as a tuple, then each
# keyword-only value (`_MISSING` when absent). Twins of VM operations are
# named `_op_<name>`. Helpers from the engine (`_dict_probe`, ...) and
# `_MISSING` are installed before this module runs. Its frames are hidden
# from tracebacks.

# --------------------------------------------------------------- keys


def _khash(k, what):
    # CPython 3.14's message for a key that cannot be hashed names the
    # key's type and the TypeError its hash raised, a tuple's item's too
    # (`Vm::hash_as`), with no context.
    t = type(k)
    if t is list or t is dict or t is set or getattr(t, '__hash__', 0) is None:
        n = _type_name(k)
        raise TypeError(f"cannot use '{n}' as a {what} (unhashable type: '{n}')")
    try:
        return hash(k)
    except TypeError as e:
        msg = str(e)
    raise TypeError(f"cannot use '{_type_name(k)}' as a {what} ({msg})")


def _dfind(d, k, h):
    # As `_sfind`: the keys only Python can compare, a bounded list at a
    # time (`dict_probe_py`).
    r = _dict_probe(d, k, h, None)
    while type(r) is not int:
        at = None
        if type(r) is tuple:
            r, at = r
        for e, key in r:
            if key is k or key == k:
                return e
        if at is None:
            return -1
        r = _dict_probe(d, k, h, at)
    return r


def _dlookup(d, k):
    e = _dfind(d, k, _khash(k, 'dict key'))
    if e < 0:
        return False, None
    return True, _dict_value_at(d, e)


def _dset(d, k, v):
    h = _khash(k, 'dict key')
    e = _dfind(d, k, h)
    if e < 0:
        _dict_insert(d, k, h, v)
    else:
        _dict_set_at(d, e, v)


def _ddel(d, k):
    e = _dfind(d, k, _khash(k, 'dict key'))
    if e < 0:
        return False, None
    v = _dict_value_at(d, e)
    _dict_del_at(d, e)
    return True, v


def _set_hashed(s):
    # A set's elements with their stored hashes, a slice per native call
    # (`_set_hashed_at`), each a safepoint.
    i = 0
    while True:
        r = _set_hashed_at(s, i)
        if r is None:
            return
        xs, i = r
        for p in xs:
            yield p


def _sfind(s, x, h, d=-1):
    # The keys only Python can compare come a bounded list at a time, with
    # where the probe goes on (`set_probe_py`). Inside a comparison at
    # nesting `d` (see `_eq`), they compare by `_eqd`.
    r = _set_probe(s, x, h, d if d > 0 else 0, None)
    while type(r) is not int:
        at = None
        if type(r) is tuple:
            r, at = r
        for i, key in r:
            if key is x or (key == x if d < 0 else _eqd(key, x, d + 1)):
                return i
        if at is None:
            return -1
        r = _set_probe(s, x, h, d if d > 0 else 0, at)
    return r


def _sadd(s, x):
    h = _khash(x, 'set element')
    if _sfind(s, x, h) < 0:
        _set_insert(s, x, h)


def _sadd_h(s, x, h):
    if _sfind(s, x, h) < 0:
        _set_insert(s, x, h)


def _ssub(a, b, d=-1):
    # Is set `a` a subset of set `b`? (stored hashes, no rehashing) Inside
    # a comparison at nesting `d` (see `_eq`), elements compare by `_eqd`.
    if len(a) > len(b):
        return False
    # Native slices of `a` (`_more`, `pl_set_scan`); the elements between
    # them that only Python can find in `b` are found here.
    r = _more(a, b, 0, d)
    while r is not None:
        if r is False:
            return False
        if type(r) is int:
            r = _more(a, b, r, d)
            continue
        j, x, h = r
        if _sfind(b, x, h, d) < 0:
            return False
        r = _more(a, b, j + 1, d)
    return True


def _shas(s, x):
    return _sfind(s, x, _khash(x, 'set element')) >= 0


def _sdiscard(s, x):
    i = _sfind(s, x, _khash(x, 'set element'))
    if i < 0:
        return False
    _set_table(s, i)
    return True


# ------------------------------------------------------------ equality


def _eq(a, b, d=0):
    # `a == b` for builtin containers at nesting `d` of a comparison the
    # native walk handed off: items go to `_eqd` at `d + 1`, so the nesting
    # carries and the walk's depth limit holds across the levels run here.
    ta = type(a)
    tb = type(b)
    if (ta is list and tb is list) or (ta is tuple and tb is tuple):
        if len(a) != len(b):
            return False
        n = len(a)
        # Native slices; the items between them compare here.
        i = _seq_scan(a, b, 0, d)
        while i < n:
            x = a[i]
            y = b[i]
            if not (x is y or _eqd(x, y, d + 1)):
                return False
            i = _seq_scan(a, b, i + 1, d)
        return True
    if ta is dict and tb is dict:
        if len(a) != len(b):
            return False
        e = _dict_scan(a, b, 0, d)
        while True:
            kv = _dict_at(a, e)
            if kv is None:
                return True
            if kv is not False:
                k, v = kv
                found, w = _dlookup(b, k)
                if not found:
                    return False
                if not (v is w or _eqd(v, w, d + 1)):
                    return False
            e = _dict_scan(a, b, e + 1, d)
    if (ta is set or ta is frozenset) and (tb is set or tb is frozenset):
        return len(a) == len(b) and _ssub(a, b, d)
    if ta is _KEYS or ta is _ITEMS or tb is _KEYS or tb is _ITEMS:
        if ta is not _KEYS and ta is not _ITEMS:
            # A set on the left: the view's reflected `==`.
            a, b = b, a
        # CPython's `all_contained_in` over any set-like `b` (a set, or a
        # keys or items view): items need not be hashable.
        if not _setlike(type(_bx_value(b))) or len(a) != len(b):
            return False
        for x in a:
            if x not in b:
                return False
        return True
    # Builtin-type subclasses (the walk saw through them) compare as the
    # values they extend.
    x = _bx_value(a)
    y = _bx_value(b)
    if x is not a or y is not b:
        return _eq(x, y, d)
    return False


def _eqd(x, y, d):
    # Items `x == y` at nesting `d`: natively if one walk decides it, else
    # one more level here; user `__eq__` methods run through `==`.
    r = _walk_eq(x, y, d)
    if r is not None:
        return r
    tx = type(x)
    ty = type(y)
    if (((tx is list or tx is tuple or tx is dict) and tx is ty)
            or ((tx is set or tx is frozenset) and (ty is set or ty is frozenset))):
        return _eq(x, y, d)
    return x == y


def _ord(a, b, d):
    # The order of two lists or two tuples at nesting `d` (-1, 0, 1, or 2
    # if unordered), found in one descent: `==` and then `<` at each level
    # would walk the rest of the data again per level. None if a pair needs
    # Python code; CPython's two steps then decide.
    n = len(a)
    if len(b) < n:
        n = len(b)
    i = _seq_scan(a, b, 0, d)
    while i < n:
        x = a[i]
        y = b[i]
        c = _walk_ord(x, y, d + 1)
        if c is None:
            t = type(x)
            if (t is list or t is tuple) and type(y) is t:
                c = _ord(x, y, d + 1)
        if c is None or c is NotImplemented:
            return None
        if c != 0:
            return c
        i = _seq_scan(a, b, i + 1, d)
    return (len(a) > len(b)) - (len(a) < len(b))


# One frame per level of a comparison the native walk handed off.
_op_eq = _eq


def _op_ne(a, b):
    return not _eq(a, b)


def _eq_bool(a, b):
    return True if a == b else False


def _cmp1(x, y, op):
    if op == 29:
        return x < y
    if op == 30:
        return x <= y
    if op == 31:
        return x > y
    return x >= y


_SYMS = {29: '<', 30: '<=', 31: '>', 32: '>='}


def _op_cmp(a, b, op, d=0):
    ta = type(a)
    tb = type(b)
    if (ta is list and tb is list) or (ta is tuple and tb is tuple):
        c = _ord(a, b, d)
        if c == 2:
            return False
        if c is not None:
            if op == 29:
                return c < 0
            if op == 30:
                return c <= 0
            if op == 31:
                return c > 0
            return c >= 0
        n = len(a)
        if len(b) < n:
            n = len(b)
        i = _seq_scan(a, b, 0, d)
        while i < n:
            x = a[i]
            y = b[i]
            if not (x is y or _eqd(x, y, d + 1)):
                # `_cmp1`, inline: one frame per level.
                if op == 29:
                    return x < y
                if op == 30:
                    return x <= y
                if op == 31:
                    return x > y
                return x >= y
            i = _seq_scan(a, b, i + 1, d)
        return _cmp1(len(a), len(b), op)
    if (ta is set or ta is frozenset) and (tb is set or tb is frozenset):
        if op == 29:
            return len(a) < len(b) and _ssub(a, b, d)
        if op == 30:
            return _ssub(a, b, d)
        if op == 31:
            return len(b) < len(a) and _ssub(b, a, d)
        return _ssub(b, a, d)
    if (ta is _KEYS or ta is _ITEMS or tb is _KEYS or tb is _ITEMS) and (
            _setlike(ta) or isinstance(a, set)) and (_setlike(tb) or isinstance(b, set)):
        # A set-like dict view and a set, or a set subclass's instance:
        # CPython's `all_contained_in`.
        if op > 30:
            a, b = b, a
            op -= 2
        if len(a) > len(b) or (op == 29 and len(a) == len(b)):
            return False
        for x in a:
            if x not in b:
                return False
        return True
    # Builtin-type subclasses (the walk saw through them) order as the
    # values they extend; errors name the subclasses.
    x = _bx_value(a)
    y = _bx_value(b)
    if x is not a or y is not b:
        tx = type(x)
        if (tx is type(y) and (tx is list or tx is tuple)) or (_setlike(tx) and _setlike(type(y))):
            return _op_cmp(x, y, op, d)
    _cmp_error(a, b, _SYMS[op])


_KEYS = type({}.keys())
_ITEMS = type({}.items())


def _setlike(t):
    return t is set or t is frozenset or t is _KEYS or t is _ITEMS


def _op_contains(c, x):
    t = type(c)
    if t is dict:
        return _dfind(c, x, _khash(x, 'dict key')) >= 0
    if t is set or t is frozenset:
        return _shas(c, x)
    tn = _type_name(c)
    if tn == 'dict_keys':
        # `_dict_probe` reads a keys or items view's dict.
        return _dfind(c, x, _khash(x, 'dict key')) >= 0
    if tn == 'dict_items':
        # As CPython: look the key up by its hash, then compare the value.
        if type(x) is not tuple or len(x) != 2:
            return False
        found, v = _dlookup(c, x[0])
        return found and (v is x[1] or v == x[1])
    for y in c:
        if y is x or y == x:
            return True
    return False


def _op_not_contains(c, x):
    return not _op_contains(c, x)


# ---------------------------------------------------------- subscripts


def _has_index(x):
    # An int subclass's `__index__` is never called for its value, as in
    # CPython's PyNumber_Index (see `_int_value`).
    return getattr(type(x), '__index__', None) is not None and not isinstance(x, int)


def _int_value(x):
    # The value of int (or int subclass) `x` as an exact int, running none
    # of its methods: CPython's PyNumber_Index on an int. enumerate's start
    # is read natively, at any size, where `int(x)` would call a subclass's
    # `__int__`.
    return next(enumerate((0,), x))[0]


def _index(x):
    r = type(x).__index__(x)
    if type(r) is not int and type(r) is not bool:
        if not isinstance(r, int):
            raise TypeError(f'__index__ returned non-int (type {_type_name(r)})')
        # An int subclass's instance (CPython warns, and takes its value).
        r = _bx_value(r)
    return r


def _key(k):
    # A subscript with `__index__` objects as (slice) indices.
    if type(k) is slice:
        a = k.start
        b = k.stop
        c = k.step
        return slice(_index(a) if _has_index(a) else a, _index(b) if _has_index(b) else b,
                     _index(c) if _has_index(c) else c)
    if _has_index(k):
        r = _index(k)
        if not -9223372036854775808 <= r <= 9223372036854775807:
            raise IndexError(f"cannot fit '{_type_name(k)}' into an index-sized integer")
        return r
    return k


def _op_getitem(o, k):
    if type(o) is dict:
        found, v = _dlookup(o, k)
        if not found:
            raise KeyError(k)
        return v
    if isinstance(o, dict):
        # A dict subclass without its own `__getitem__` (the dispatch loop
        # calls that) and a key whose hash or `==` is Python code: `o[k]`
        # would come back here. `dict.get` looks the key up once; this
        # function is the default no key can be.
        v = dict.get(o, k, _op_getitem)
        if v is _op_getitem:
            m = getattr(type(o), '__missing__', None)
            if m is None:
                raise KeyError(k)
            return m(o, k)
        return v
    return o[_key(k)]


def _op_setitem(o, k, v):
    if type(o) is dict:
        _dset(o, k, v)
        return
    if isinstance(o, dict):
        dict.__setitem__(o, k, v)
        return
    k = _key(k)
    if type(k) is slice and isinstance(o, list) and type(v) is not list:
        # The items are taken before the slice is resolved, as CPython's
        # `PySequence_Fast` does.
        try:
            it = iter(v)
        except TypeError:
            raise TypeError('must assign iterable to extended slice') from None
        v = list(it)
    elif type(o) is bytearray:
        if type(k) is slice:
            if type(v) is not bytes and type(v) is not bytearray and type(v) not in (int, bool, float, str, complex):
                v = _ba_items(v)
        elif _has_index(v):
            v = _index(v)
    o[k] = v


def _ba_items(v):
    # A slice assignment's iterable of ints with `__index__` objects or a
    # generator, or a long list, tuple or set, as bytes.
    if type(v) in (list, tuple, set, frozenset):
        try:
            return bytearray(v)
        except TypeError:
            pass
    try:
        it = iter(v)
    except TypeError:
        raise TypeError(f"cannot convert '{_type_name(v)}' object to bytearray") from None
    return bytes([_index(x) if _has_index(x) else x for x in it])


def _op_delitem(o, k):
    if type(o) is dict:
        found, v = _ddel(o, k)
        if not found:
            raise KeyError(k)
        return
    if isinstance(o, dict):
        dict.__delitem__(o, k)
        return
    del o[_key(k)]


def _op_build_set(items):
    s = set()
    for x in items:
        _sadd(s, x)
    return s


def _op_build_dict(flat):
    d = {}
    i = 0
    n = len(flat)
    while i < n:
        _dset(d, flat[i], flat[i + 1])
        i += 2
    return d


def _op_store_attr(o, name, v):
    # An exception's `args = v` met Python code: an iterator that runs it
    # (collected here), or the complete list of one that went on in chunks
    # (`_collect_rest`).
    setattr(o, name, v if type(v) is list else tuple(v))


def _op_set_add(s, x):
    _sadd(s, x)


def _op_set_update(s, v, in_call):
    for x in v:
        _sadd(s, x)


def _op_list_extend(l, v, in_call):
    if type(v) is list:
        # A complete list from `_collect_rest`: one native extend.
        l.extend(v)
        return
    for x in v:
        l.append(x)


def _op_dict_update(d, m, in_call, f):
    if isinstance(m, dict) and type(m).__iter__ is dict.__iter__:
        # CPython copies a dict subclass that keeps dict's iteration
        # directly, without its `keys()` or `__getitem__`.
        m = _bx_value(m)
        if not in_call:
            d.update(m)
            return
    if not hasattr(type(m), "keys"):
        if in_call:
            raise TypeError(
                f"{f} argument after ** must be a mapping, not {type(m).__name__}"
            )
        raise TypeError(f"'{type(m).__name__}' object is not a mapping")
    for k in m.keys():
        if in_call:
            if not isinstance(k, str):
                raise TypeError("keywords must be strings")
            if k in d:
                raise TypeError(f"{f} got multiple values for keyword argument '{k}'")
        d[k] = m[k]


def _op_unpack(v, n):
    out = []
    for x in v:
        if len(out) == n:
            raise ValueError(f"too many values to unpack (expected {n})")
        out.append(x)
    if len(out) < n:
        raise ValueError(f"not enough values to unpack (expected {n}, got {len(out)})")
    return out


def _op_unpack_ex(v, before, after):
    if type(v) is list:
        items = list(v)
    else:
        items = []
        for x in v:
            items.append(x)
    n = len(items)
    if n < before + after:
        raise ValueError(f"not enough values to unpack (expected at least {before + after}, got {n})")
    return items[:before] + [items[before:n - after]] + items[n - after:]


def _op_fmt(v, conv, spec):
    if conv == 114:
        v = repr(v)
    elif conv == 115:
        v = str(v)
    elif conv == 97:
        v = ascii(v)
    r = format(v, spec)
    # A lone f-string field (conversion 1) is what `__format__` returned;
    # a joined one is its exact str, as `BUILD_STRING` reads only strs.
    if conv != 1 and type(r) is not str:
        r = str.__str__(r)
    return r


def _op_unary(v, op):
    _cmp_error(v, v, 'unary')


def _op_iter(v):
    return _seq_iter(v)


def _await_step(it, v):
    # One step of an `await` that delegates to an `__await__()` iterator:
    # a value is a yield, StopIteration the result (as `yield from`).
    if v is None:
        return next(it)
    return it.send(v)


def _await_throw(it, e):
    # `throw()` into a coroutine delegating to `it`, as `yield from` does.
    if isinstance(e, GeneratorExit):
        close = getattr(it, 'close', None)
        if close is not None:
            close()
        raise e
    throw = getattr(it, 'throw', None)
    if throw is None:
        raise e
    return throw(e)


def _bad_yield(y):
    raise RuntimeError(f'Task got bad yield: {y!r}')


def _coro_close(c):
    try:
        c.throw(GeneratorExit)
    except GeneratorExit:
        return None
    except StopIteration as e:
        return e.value
    raise RuntimeError('coroutine ignored GeneratorExit')


def _coro_wrapper(c):
    return coroutine_wrapper(c)


async def _anext_default(it, default):
    try:
        return await it.__anext__()
    except StopAsyncIteration:
        return default


def _seq_iter(v):
    i = 0
    while True:
        try:
            x = v[i]
        except (IndexError, StopIteration):
            return
        yield x
        i += 1


def _op_binop(a, b, op, inplace):
    ta = type(a)
    if op == 10 and (isinstance(a, type) or isinstance(b, type)):
        return _union_or(a, b)
    if op == 0 and inplace and isinstance(a, list):
        list.extend(a, b)
        return a
    view = ta is _KEYS or ta is _ITEMS
    if (op == 1 or 9 <= op <= 11) and (view or type(b) is _KEYS or type(b) is _ITEMS) and not (
            inplace and op == 10 and isinstance(a, dict)):
        # A set-like dict view with any iterable: plain sets, as CPython's
        # `dictviews_*` make them.
        if op == 9:
            # An exact set at least as long as the view reads the view; of
            # two views the longer is probed.
            v, o = (a, b) if view else (b, a)
            if type(o) is set and len(v) <= len(o):
                return set.intersection(o, v)
            if (type(o) is _KEYS or type(o) is _ITEMS) and len(o) > len(v):
                v, o = o, v
            out = set()
            for x in o:
                if x in v:
                    out.add(x)
            return out
        if op == 11 and ta is _ITEMS and type(b) is _ITEMS:
            # `dictitems_xor`: the pairs of `b` that `a` lacks, then those
            # of `a` that `b` did not match.
            out = set()
            hit = set()
            for k, v in b:
                e = _dfind(a, k, _khash(k, 'dict key'))
                if e >= 0:
                    w = _dict_value_at(a, e)
                    if w is v or w == v:
                        hit.add(e)
                        continue
                out.add((k, v))
            e = 0
            p = _dict_at(a, 0)
            while p is not None:
                if p is not False and e not in hit:
                    out.add(p)
                e += 1
                p = _dict_at(a, e)
            return out
        # `dictviews_to_set`: a keys view presizes as its dict does.
        out = set()
        if ta is _KEYS:
            _set_table(out, (a,))
        set.update(out, a)
        if op == 10:
            set.update(out, b)
        elif op == 1:
            set.difference_update(out, b)
        else:
            set.symmetric_difference_update(out, b)
        return out
    # Sets and their subclasses: the values they extend; an in-place
    # operator returns its left operand.
    ua = _bx_value(a)
    ub = _bx_value(b)
    tu = type(ua)
    if (tu is set or tu is frozenset) and (type(ub) is set or type(ub) is frozenset):
        out = _op_setop(ua, ub, op, inplace and tu is set)
        if out is not None:
            if inplace and out is ua:
                return a
            return frozenset(out) if tu is frozenset else out
    # A dict subclass's instance as its dict (`dict_or`, `dict_ior`).
    if tu is dict and op == 10 and inplace and type(ub) is not dict:
        dict.update(ua, b)
        return a
    if tu is dict and type(ub) is dict and op == 10:
        out = ua if inplace else _dict((ua,), _MISSING)
        for k, v in ub.items():
            _dset(out, k, v)
        return a if inplace else out
    if op == 5 and (tu is str or tu is bytes):
        # The format's value; the right operand as given (a str subclass's
        # `__str__`, a mapping's `__getitem__`).
        return _percent(ua, b)
    if op == 2 and _has_index(b):
        n = _index(b)
        if not -9223372036854775808 <= n <= 9223372036854775807:
            raise OverflowError(f"cannot fit '{_type_name(b)}' into an index-sized integer")
        # `*=` repeats a list (or its subclass's) or a bytearray in place.
        u = _bx_value(a)
        if inplace and (type(u) is list or type(u) is bytearray):
            u *= n
            return a
        return a * n
    if op == 2 and _has_index(a):
        # `n *= seq`: a class defined in Python has no repetition of its
        # own, so an exact sequence refuses; a subclass's `__rmul__`
        # repeats it into a new object.
        tb = type(b)
        if inplace and (tb is list or tb is tuple or tb is str or tb is bytes or tb is bytearray):
            raise TypeError(f"unsupported operand type(s) for *=: '{_type_name(a)}' and '{_type_name(b)}'")
        n = _index(a)
        if not -9223372036854775808 <= n <= 9223372036854775807:
            raise OverflowError(f"cannot fit '{_type_name(a)}' into an index-sized integer")
        return n * b
    if op == 0:
        return a + b
    _cmp_error(a, b, 'binary operator')


def _op_setop(a, b, op, inplace):
    # Stored hashes, and CPython's probe direction (the table entry is the
    # left operand of `==`).
    if op == 10:
        out = set(a) if not inplace else a
        # `set_merge`: presized, as the native is; `a | a` is a copy.
        if b is not a:
            set.update(out, b)
        return out
    out = set()
    if op == 9 and b is a:
        # `set_intersection` of a set with itself: a copy.
        out = set(a)
    elif op == 9:
        so, other = (b, a) if len(a) > len(b) else (a, b)
        for x, h in _set_hashed(other):
            if _sfind(so, x, h) >= 0:
                _sadd_h(out, x, h)
    elif op == 1:
        for x, h in _set_hashed(a):
            if _sfind(b, x, h) < 0:
                _sadd_h(out, x, h)
    elif op == 11:
        if inplace:
            # `set_symmetric_difference_update`: `a` itself, each item of
            # `b` toggled.
            if b is a:
                a.clear()
                return a
            for x, h in _set_hashed(b):
                i = _sfind(a, x, h)
                if i >= 0:
                    _set_table(a, i)
                else:
                    _set_insert(a, x, h)
            return a
        for x, h in _set_hashed(a):
            if _sfind(b, x, h) < 0:
                _sadd_h(out, x, h)
        for x, h in _set_hashed(b):
            if _sfind(a, x, h) < 0:
                _sadd_h(out, x, h)
        return out
    else:
        return None
    if inplace:
        a.clear()
        for x in out:
            _sadd(a, x)
        return a
    return out


def _percent(fmt, args):
    # `fmt % args` with values only Python code can convert: `_pct` reruns
    # the format with the conversions done so far and asks for the next.
    pre = []
    while True:
        r = _pct(fmt, args, pre)
        if type(r) is not tuple:
            return r
        v, c = r
        if c == 's':
            x = str(v)
        elif c == 'r':
            x = repr(v)
        elif c == 'a':
            x = ascii(v)
        elif c == 'b':
            x = type(v).__bytes__(v)
            if not isinstance(x, bytes):
                raise TypeError('__bytes__ returned non-bytes (type ' + _type_name(x) + ')')
        elif c == 'i':
            # PyNumber_Long: `__int__` before `__index__`.
            x = int(v)
        elif c == 'x':
            x = _index(v)
        elif c == 'f':
            x = float(v)
        elif c == 'p':
            x = format(v[0], v[1])
        else:
            x = v[0][v[1]]
        pre.append(x)


# ---------------------------------------------------------------- repr

_REPR_ACTIVE = []
# The ids of `_REPR_ACTIVE`, so meeting one again costs one lookup, not a
# scan per level (a deep nesting parked level by level would be quadratic).
_REPR_IDS = {}


# Builtin containers to render here before trying a native walk again, and
# the gap after the next walk that fails (see `_repr_to`).
_REPR_SKIP = [0, 1]


def _repr_of(x, d=0):
    # The pieces of every level go to one list, joined once: a level that
    # joined its own text built a string the size of its subtree, so a deep
    # nesting built quadratic text (30,000 nested dicts: 32 s [M]). `out[0]`
    # holds the runs `_repr_run` has joined so far.
    if not d:
        _REPR_SKIP[0] = 0
        _REPR_SKIP[1] = 1
    out = [[]]
    _repr_to(x, d, out)
    _repr_run(out)
    return ''.join(out[0])


def _repr_run(out):
    # Joins the pieces after `out[0]` into one run: a long container's text
    # then lives as a few long strs, not millions of short ones for every
    # collection to mark (a 1M-key dict: longest step 98-122 -> 16 ms, and
    # `f'{d}'` fits in 192 MiB, not 384 [M]).
    runs = out.pop(0)
    runs.append(''.join(out))
    out.clear()
    out.append(runs)


def _repr_to(x, d, out):
    b = _bx_value(x)
    if b is not x:
        # A builtin-type subclass without its own __repr__; a set subclass
        # shows its class, as CPython's set_repr.
        if type(b) is set:
            if not b:
                out.append(type(x).__name__ + '()')
                return
            out.append(type(x).__name__ + '(')
            _repr_to(b, d, out)
            out.append(')')
            return
        _repr_to(b, d, out)
        return
    t = type(x)
    tn = _type_name(x)
    view = tn == 'dict_keys' or tn == 'dict_values' or tn == 'dict_items'
    if t is list or t is tuple or t is set or t is frozenset or t is dict or view:
        if id(x) in _REPR_IDS:
            if t is list:
                out.append('[...]')
            elif t is dict:
                out.append('{...}')
            else:
                out.append('...')
            return
        _REPR_ACTIVE.append(x)
        _REPR_IDS[id(x)] = x
        try:
            it = None
            if t is dict:
                # Entry by entry, seeing insertions made by the reprs.
                out.append('{')
                end = '}'
            elif t is list:
                out.append('[')
                end = ']'
                it = iter(x)
            elif t is tuple:
                out.append('(')
                end = ',)' if len(x) == 1 else ')'
                it = iter(x)
            elif t is set or t is frozenset:
                if not x:
                    out.append(tn + '()')
                    return
                out.append('{' if t is set else 'frozenset({')
                end = '}' if t is set else '})'
                it = iter(list(x))
            else:
                out.append(tn + '([')
                end = '])'
                it = iter(x)
            # Each item inline, so a level of nesting costs one frame:
            # prelude frames stop at ten times the recursion limit.
            e = 0
            first = True
            while True:
                if it is None:
                    p = _dict_at(x, e)
                    if p is None:
                        break
                    e += 1
                    if not p:
                        continue
                else:
                    y = next(it, _REPR_ACTIVE)
                    if y is _REPR_ACTIVE:
                        break
                    p = (y,)
                if not first:
                    out.append(', ')
                first = False
                k = False
                for y in p:
                    if k:
                        out.append(': ')
                    k = True
                    # One native walk with the fuel left, else one more level
                    # here (a builtin container) or `repr` (user `__repr__`
                    # methods run through it).
                    ty = type(y)
                    nest = ty is list or ty is tuple or ty is set or ty is frozenset or ty is dict
                    if not nest:
                        tm = _type_name(y)
                        nest = tm == 'dict_keys' or tm == 'dict_values' or tm == 'dict_items'
                    if nest and _REPR_SKIP[0]:
                        _REPR_SKIP[0] -= 1
                        _repr_to(y, d + 1, out)
                        continue
                    r = _walk_repr(y, d + 1, _REPR_IDS)
                    if r is not None:
                        if nest:
                            _REPR_SKIP[1] = 1
                        out.append(r)
                    elif nest:
                        # The walk ran out of budget: the next 1, 2, 4, ...
                        # (at most 1024) containers are rendered here without
                        # one, so a deep nesting wastes about log2(depth)
                        # budgets, not one per level, and descends here at
                        # most 1024 levels deeper than a walk per level would.
                        _REPR_SKIP[0] = _REPR_SKIP[1]
                        if _REPR_SKIP[1] < 1024:
                            _REPR_SKIP[1] *= 2
                        _repr_to(y, d + 1, out)
                    else:
                        out.append(repr(y))
                if len(out) > 4096:
                    _repr_run(out)
            out.append(end)
        finally:
            del _REPR_IDS[id(_REPR_ACTIVE.pop())]
        return
    if t is str or t is bytes or t is bytearray:
        # A long one, which `repr` builds a step at a time.
        out.append(repr(x))
    elif isinstance(x, BaseException):
        args = x.args
        if len(args) == 1:
            out.append(tn + '(' + repr(args[0]) + ')')
        else:
            out.append(tn + repr(args))
    elif t is Task or t is _AioFuture:
        out.append(_aio_repr(x))
    elif tn == 'method':
        out.append('<bound method ' + x.__func__.__qualname__ + ' of ' + repr(x.__self__) + '>')
    elif tn == 'staticmethod' or tn == 'classmethod':
        out.append('<' + tn + '(' + repr(x.__func__) + ')>')
    else:
        out.append(object.__repr__(x))


def _exc_str(e):
    args = e.args
    if len(args) == 0:
        return ''
    if len(args) == 1:
        if isinstance(e, KeyError):
            return repr(args[0])
        return str(args[0])
    if isinstance(e, OSError) and len(args) <= 5:
        s = '[Errno ' + str(args[0]) + '] ' + str(args[1])
        if len(args) > 2 and args[2] is not None:
            s += ': ' + repr(args[2])
            if len(args) > 4 and args[4] is not None:
                s += ' -> ' + repr(args[4])
        return s
    return repr(args)


def _repr(a):
    return _repr_of(a[0])


def _ascii(a):
    # backslashreplace writes ascii()'s escapes; no list of chars, so a
    # long container's text stays one str.
    r = repr(a[0])
    if r.isascii():
        return r
    return r.encode('ascii', 'backslashreplace').decode('ascii')


def _str(a):
    if len(a) == 0:
        return ''
    x = a[0]
    if isinstance(x, BaseException):
        return _exc_str(x)
    return _repr_of(x)


def _format(a):
    v = a[0]
    spec = a[1] if len(a) > 1 else ''
    if spec == '':
        return str(v)
    return format(v, spec)


def _print(a, sep, end, file, flush):
    if file is _MISSING or file is None:
        import sys
        file = sys.stdout
        if file is None:
            return None
    if file is not _MISSING and file is not None and type(file).__name__ != 'TextIOWrapper':
        # print(..., file=obj) writes each piece with obj.write, as CPython.
        sep = ' ' if sep is _MISSING or sep is None else sep
        end = '\n' if end is _MISSING or end is None else end
        for s, what in ((sep, 'sep'), (end, 'end')):
            if not isinstance(s, str):
                raise TypeError(what + ' must be None or a string, not ' + type(s).__name__)
        write = file.write
        for i, x in enumerate(a):
            if i:
                write(sep)
            write(x if type(x) is str else str(x))
        write(end)
        if flush is not _MISSING and flush:
            file.flush()
        return None
    kw = {}
    if sep is not _MISSING:
        kw['sep'] = sep
    if end is not _MISSING:
        kw['end'] = end
    if file is not _MISSING:
        kw['file'] = file
    if flush is not _MISSING:
        kw['flush'] = flush
    parts = []
    try:
        for x in a:
            parts.append(x if type(x) is str else str(x))
    except BaseException:
        # CPython has written what precedes the failing object (and its
        # separator) by then; output stays whole when printing itself fails.
        if parts:
            kw['end'] = ' ' if sep is _MISSING or sep is None else sep
            print(*parts, **kw)
        raise
    print(*parts, **kw)


def _complex_part(x):
    t = type(x)
    if hasattr(t, '__complex__'):
        c = t.__complex__(x)
        if not isinstance(c, complex):
            raise TypeError('__complex__ returned non-complex (type ' + type(c).__name__ + ')')
        return c
    if hasattr(t, '__float__'):
        return float(x)
    if hasattr(t, '__index__'):
        return float(t.__index__(x))
    return x


def _complex(a):
    if len(a) > 1:
        return complex(_complex_part(a[0]), _complex_part(a[1]))
    return complex(_complex_part(a[0]))


def _hash(a):
    x = a[0]
    if type(x) is tuple:
        return _thash(x)
    return hash(x)


def _thash(x):
    # A tuple's hash, each distinct inner tuple hashed once (CPython caches
    # tuple hashes), so shared nesting is linear here and parks. A long
    # tuple is hashed a native slice per call (`_hash_tuple`), each call a
    # safepoint, an item whose hash needs Python code here.
    memo = {}
    if len(x) < 16384:
        hs = []
        for y in x:
            if type(y) is tuple:
                h = memo.get(id(y))
                if h is None:
                    h = hash(y)
                    memo[id(y)] = h
            else:
                h = hash(y)
            hs.append(h)
        return _hash_tuple(x, 0, None, hs)
    r = _hash_tuple(x, 0, None, None)
    while type(r) is tuple:
        i, acc = r
        y = x[i]
        if type(y) is tuple:
            h = memo.get(id(y))
            if h is None:
                h = hash(y)
                memo[id(y)] = h
        else:
            h = hash(y)
        r = _hash_tuple(x, i, acc, h)
    return r


def _BaseException___str__(a):
    return _exc_str(a[0])


def _BaseException___repr__(a):
    return _repr_of(a[0])


def _object___repr__(a):
    return _repr_of(a[0])


# ------------------------------------------------------------ builtins


def _invoke(f, args, kw):
    if kw is None:
        return f(*args)
    return f(*args, **kw)


def _mreal(x):
    t = type(x)
    if t is float or t is int or t is bool:
        return x
    if getattr(t, '__float__', None) is not None:
        return float(x)
    return float(_index(x)) if _has_index(x) else x


_MATH_INTS = ('math.factorial', 'math.gcd', 'math.lcm', 'math.isqrt', 'math.comb', 'math.perm')


def _mcall(f, args, kw):
    new = []
    changed = False
    # math's functions of reals take what __float__ (else __index__) gives.
    name = _native_name(f)
    # prod and sumprod of user numbers (Fraction, Decimal) run their
    # operators; dist takes what __float__ gives.
    if name == 'math.prod':
        acc = kw['start'] if 'start' in kw else 1
        for x in args[0]:
            acc = acc * x
        return acc
    if name == 'math.sumprod':
        # Pairs pulled one at a time, as CPython's: the native sums those
        # before the first pair that is not two numbers, then `+` and `*`.
        p = iter(args[0])
        q = iter(args[1])
        ps = []
        qs = []
        total = _MISSING
        for x in p:
            y = next(q, _MISSING)
            if y is _MISSING:
                raise ValueError('Inputs are not the same length')
            if total is _MISSING:
                tx = type(x)
                ty = type(y)
                if (tx is int or tx is float or tx is bool) and (ty is int or ty is float or ty is bool):
                    ps.append(x)
                    qs.append(y)
                    continue
                total = f(ps, qs)
            total = total + x * y
        if next(q, _MISSING) is not _MISSING:
            raise ValueError('Inputs are not the same length')
        return f(ps, qs) if total is _MISSING else total
    if name == 'math.dist':
        return f([_mreal(x) for x in args[0]], [_mreal(x) for x in args[1]])
    if (name == 'math.gcd' or name == 'math.lcm') and len(args) > 2:
        # Pairwise, as CPython's loop: each pair a native call, with its
        # own estimate and step.
        acc = 0 if name == 'math.gcd' else 1
        for x in args:
            acc = f(acc, x)
        return acc
    if name == 'math.lcm' and len(args) == 2 and type(args[0]) in (int, bool) and type(args[1]) in (int, bool):
        # Two long ints: the gcd, the short operand's division and the
        # product, each its own native step.
        a = abs(args[0])
        b = abs(args[1])
        if a < b:
            a, b = b, a
        if not b:
            return 0
        return a * (b // __import__('math').gcd(a, b))
    real = name[:5] == 'math.' and name not in _MATH_INTS
    for a in args:
        if name == 'math.fsum':
            new.append(_pulled(a, f, _mreal, _is_float))
            changed = True
        elif _lazy(a, 1):
            if name == 'bytes' or name == 'bytearray':
                a = _pulled(a, f, None, _is_byte)
            elif name == 'extend':
                a = _pulled(a, bytearray, None, _is_byte)
            else:
                a = list(a)
            new.append(a)
            changed = True
        elif real and type(a) not in (int, bool, float) and (
                getattr(type(a), '__float__', None) is not None or _has_index(a)):
            new.append(_mreal(a))
            changed = True
        elif _has_index(a):
            new.append(_index(a))
            changed = True
        else:
            new.append(a)
    if not changed:
        raise TypeError('micro-monty does not support user-defined objects in ' + name.split('.')[-1] + '()')
    return f(*new, **kw)


def _list(a):
    return [x for x in a[0]]


def _pulled(it, f, conv, ok):
    # The items of `it` (each `conv(x)`) for native `f`, pulled one at a
    # time: an item `ok` does not pass goes through `f((x,))` first, which
    # raises the native's error for it, so `it` stops where CPython's
    # does.
    xs = []
    for x in it:
        if conv is not None:
            x = conv(x)
        if not ok(x):
            f((x,))
        xs.append(x)
    return xs


def _is_float(x):
    return type(x) is float


def _is_byte(x):
    return type(x) is int and 0 <= x <= 255


def _collect_rest(xs, it, f, a, kw):
    # A native `collect` stopped part way through `it`, which need not end
    # (docs/spec.md §5): `xs` holds what it took and stands in for `it` in
    # `a`. The rest comes a chunk per loop step, each a safepoint, then `f`
    # goes on with the complete list.
    while _more(xs, it):
        pass
    return f(*a, **kw)


def _steps(st):
    # A chunked native's further steps (`Vm::chunk_rest`), each a call and so
    # a safepoint (docs/spec.md §5): `_more` gives `st` back while more
    # remain.
    r = _more(st)
    while r is st:
        r = _more(st)
    return r


def _num_error(s, what):
    # `float()`/`complex()` of a long text that is not a number: CPython's
    # message holds its whole repr, built here in steps (`repr` and `+`
    # are chunked; an f-string's `!r` is one native step).
    raise ValueError('could not convert string to ' + what + ': ' + repr(s))


def _seq_has(s, x, i, neg):
    # `x in s` from item `i` on, once an item's `==` needs Python code.
    while i < len(s):
        y = s[i]
        if y is x or y == x:
            return not neg
        i += 1
    return neg


def __more(a):
    # `_more(xs, it)` met an item that needs Python code (the sort form
    # never does).
    xs, it = a
    for x in it:
        xs.append(x)
    return False


def _tuple(a):
    return tuple(_list(a))


def _set(a):
    s = set()
    if len(a):
        for x in a[0]:
            _sadd(s, x)
    return s


def _frozenset(a):
    # The set built, frozen (`_set_table(s, None)`): a copy would rehash
    # it out of CPython's order.
    return _set_table(_set(a), None)


def _pair_list(item):
    # CPython 3.14's `PySequence_Fast(item, "object is not iterable")`: a
    # TypeError from `iter` is replaced, with no context.
    try:
        it = iter(item)
    except TypeError:
        it = None
    if it is None:
        raise TypeError('object is not iterable')
    return list(it)


def _dict_fill(d, src):
    if type(src) is dict:
        for k, v in src.items():
            _dset(d, k, v)
        return
    if hasattr(src, 'keys'):
        for k in src.keys():
            _dset(d, k, src[k])
        return
    i = 0
    for item in src:
        pair = _pair_list(item)
        if len(pair) != 2:
            raise ValueError(f"dictionary update sequence element #{i} has length {len(pair)}; 2 is required")
        _dset(d, pair[0], pair[1])
        i += 1


def _dict(a, kw):
    d = {}
    if len(a):
        _dict_fill(d, a[0])
    if kw is not _MISSING:
        _dict_fill(d, kw)
    return d


def _dict_update(a, kw):
    d = a[0]
    if len(a) > 1:
        _dict_fill(d, a[1])
    if kw is not _MISSING:
        _dict_fill(d, kw)


def _dict_get(a):
    found, v = _dlookup(a[0], a[1])
    if found:
        return v
    return a[2] if len(a) > 2 else None


def _dict_pop(a):
    found, v = _ddel(a[0], a[1])
    if found:
        return v
    if len(a) > 2:
        return a[2]
    raise KeyError(a[1])


def _dict_setdefault(a):
    d = a[0]
    k = a[1]
    found, v = _dlookup(d, k)
    if found:
        return v
    v = a[2] if len(a) > 2 else None
    _dset(d, k, v)
    return v


def _set_add(a):
    _sadd(a[0], a[1])


def _set_remove(a):
    if not _sdiscard(a[0], a[1]):
        raise KeyError(a[1])


def _set_discard(a):
    _sdiscard(a[0], a[1])


def _set_update(a):
    s = a[0]
    for other in a[1:]:
        t = type(other)
        if t is set or t is frozenset or t is dict:
            # The native, which presizes `s` first as CPython's
            # `set_merge` does (its table order depends on it).
            set.update(s, other)
        elif isinstance(other, set):
            # A set subclass's instance merges as its set does.
            set.update(s, _bx_value(other))
        else:
            for x in other:
                _sadd(s, x)


def _set_union(a):
    # CPython's `set_union`: the set itself among the others is skipped.
    s = set(a[0])
    _set_update((s,) + tuple(o for o in a[1:] if o is not a[0]))
    return s


def _set_symmetric_difference(a):
    # CPython's: the set of the other's items, then each element of the
    # set toggled in.
    s = set(a[1])
    _set_symmetric_difference_update((s, a[0]))
    return s


def _set_symmetric_difference_update(a):
    # CPython's: a set's elements toggled as they are, with their hashes,
    # anything else's as the set of its items.
    s = a[0]
    o = a[1]
    if type(o) is dict:
        for x in o:
            if not _sdiscard(s, x):
                _sadd(s, x)
        return
    if type(o) is not set and type(o) is not frozenset:
        o = set(o)
    for x, h in _set_hashed(o):
        j = _sfind(s, x, h)
        if j >= 0:
            _set_table(s, j)
        else:
            _sadd_h(s, x, h)


def _sinter(s, o):
    # CPython's `set_intersection`: of two sets the smaller's elements probe
    # the larger; any other iterable's items, hashed as `hash()` does,
    # probe `s`, read no further once all of `s` is found.
    r = set()
    if type(o) is set or type(o) is frozenset:
        if len(o) > len(s):
            s, o = o, s
        for x, h in _set_hashed(o):
            if _sfind(s, x, h) >= 0:
                _sadd_h(r, x, h)
        return r
    n = len(s)
    for x in o:
        h = hash(x)
        if _sfind(s, x, h) >= 0:
            _sadd_h(r, x, h)
            if len(r) >= n:
                break
    return r


def _set_intersection_update(a):
    s = a[0]
    for other in a[1:]:
        _set_table(s, _sinter(s, other))


def _set_difference_update(a):
    s = a[0]
    for other in a[1:]:
        for x in other:
            _sdiscard(s, x)
        _set_table(s, s)


def _set_difference(a):
    # CPython's `set_difference_multi`: the first other taken as
    # `set_difference` does (a set or dict at least a quarter of `s`'s
    # size probed by each element of `s`; anything else removed from a
    # copy), then the rest removed in place.
    s = a[0]
    if len(a) > 1:
        o = a[1]
        t = type(o)
        if (t is set or t is frozenset or t is dict) and len(s) >> 2 <= len(o):
            r = set()
            for x, h in _set_hashed(s):
                if (_dfind(o, x, h) if t is dict else _sfind(o, x, h)) < 0:
                    _sadd_h(r, x, h)
            _set_difference_update((r,) + a[2:])
            return r
    r = set(s)
    _set_difference_update((r,) + a[1:])
    return r


def _set_intersection(a):
    s = a[0]
    if len(a) == 1:
        return set(s)
    for other in a[1:]:
        s = _sinter(s, other)
    return s


def _frozenset_difference(a):
    return _set_table(_set_difference(a), None)


def _frozenset_intersection(a):
    return _set_table(_set_intersection(a), None)


def _set_isdisjoint(a):
    s, o = a[0], a[1]
    if type(o) is set or type(o) is frozenset:
        if len(o) > len(s):
            s, o = o, s
        for x, h in _set_hashed(o):
            if _sfind(s, x, h) >= 0:
                return False
        return True
    for x in o:
        if _shas(s, x):
            return False
    return True


def _dict_fromkeys(a):
    v = a[2] if len(a) > 2 else None
    c = a[0] if isinstance(a[0], type) else type(a[0])
    if c is not dict:
        d = c()
        for k in a[1]:
            d[k] = v
        return d
    d = {}
    for k in a[1]:
        _dset(d, k, v)
    return d


def _set_issubset(a):
    s, o = a[0], a[1]
    if type(o) is set or type(o) is frozenset:
        return _ssub(s, o)
    return len(_sinter(s, o)) == len(s)


def _set_issuperset(a):
    s, o = a[0], a[1]
    if type(o) is set or type(o) is frozenset:
        return _ssub(o, s)
    # The first item missing from `s` decides; the rest is not read.
    for x in o:
        if not _shas(s, x):
            return False
    return True


_frozenset_issubset = _set_issubset
_frozenset_issuperset = _set_issuperset
_frozenset_isdisjoint = _set_isdisjoint


def _dict_keys_isdisjoint(a):
    # CPython's `dictviews_isdisjoint`: a longer set or view is probed by
    # the view's items, else the other's items probe the view.
    v, o = a[0], a[1]
    if v is o:
        return len(v) == 0
    to = type(o)
    if (isinstance(o, set) or isinstance(o, frozenset) or to is _KEYS or to is _ITEMS) and len(o) > len(v):
        v, o = o, v
    for x in o:
        if x in v:
            return False
    return True


_dict_items_isdisjoint = _dict_keys_isdisjoint


def _frozenset_union(a):
    return _set_table(_set_union(a), None)


def _frozenset_symmetric_difference(a):
    return _set_table(_set_symmetric_difference(a), None)


def _seq_find(seq, x, start, stop):
    n = len(seq)
    if start < 0:
        start += n
        if start < 0:
            start = 0
    if stop < 0:
        stop += n
    if stop > n:
        stop = n
    i = start
    while i < stop:
        y = seq[i]
        if y is x or y == x:
            return i
        i += 1
    return -1


def _list_index(a, t='list'):
    b = [_index(v) if _has_index(v) else v for v in a[2:4]]
    i = _seq_find(a[0], a[1], b[0] if b else 0, b[1] if len(b) > 1 else 9223372036854775807)
    if i < 0:
        raise ValueError(f'{t}.index(x): x not in {t}')
    return i


def _tuple_index(a):
    return _list_index(a, 'tuple')


def _seq_count(seq, x):
    c = 0
    for y in seq:
        if y is x or y == x:
            c += 1
    return c


def _list_count(a):
    return _seq_count(a[0], a[1])


def _tuple_count(a):
    return _seq_count(a[0], a[1])


def _list_remove(a):
    l = a[0]
    i = _seq_find(l, a[1], 0, len(l))
    if i < 0:
        raise ValueError('list.remove(x): x not in list')
    del l[i]


def _list___init__(a):
    # CPython's `list___init___impl`: emptied, then extended.
    l = a[0]
    list.clear(l)
    if len(a) > 1:
        list.extend(l, a[1])


def _set___init__(a):
    s = a[0]
    items = list(a[1]) if len(a) > 1 else []
    s.clear()
    s.update(items)


def _dict___init__(a, kw):
    _dict_update(a, kw)


def _seq_reversed(seq):
    i = len(seq) - 1
    while i >= 0:
        yield seq[i]
        i -= 1


# ---------------------------------------------------------------- dir


def _dir_sort(names):
    # `dir()`: the names in scope, from the native `dir`.
    return sorted(set(names))


def _dir(args, scope):
    # `dir(o)`: what the `__dir__` of `o`'s type lists, sorted.
    o = args[0]
    return sorted(type(o).__dir__(o))


def _object___dir__(args):
    # `object.__dir__`: the instance's `__dict__`, then its class's names.
    o = args[0]
    names = set()
    d = getattr(o, '__dict__', None)
    if type(d) is dict:
        names.update(d)
    c = getattr(o, '__class__', None)
    if c is not None:
        names.update(_type___dir__((c,)))
    return list(names)


def _type___dir__(args):
    # `type.__dir__`: the names of a class and its bases, not its metaclass.
    names = set()
    for k in args[0].__mro__:
        d = k.__dict__
        names.update(d)
        if '__module__' in d:
            # A class statement's descriptors.
            names.add('__dict__')
            if not issubclass(k, (int, str, tuple, bytes)):
                names.add('__weakref__')
        else:
            # A builtin type: what is not in its method table.
            names.update(_DIR_EXTRA.get(k.__name__, '').split())
    return list(names)


def _module___dir__(args):
    d = args[0].__dict__
    if '__dir__' in d:
        return d['__dir__']()
    return list(d) + ['__name__']


# CPython 3.14's names of builtin types that micro-monty implements as VM
# operations or data attributes, not method-table entries.
_DIR_EXTRA = {
    'object': '__class__ __doc__ __getstate__ __lt__ __le__ __gt__ __ge__ '
    '__reduce__ __reduce_ex__ __sizeof__ __subclasshook__',
    'type': '__abstractmethods__ __annotate__ __annotations__ __base__ '
    '__bases__ __basicsize__ __dict__ __dictoffset__ __flags__ '
    '__getattribute__ __instancecheck__ __itemsize__ __module__ __mro__ '
    '__name__ __or__ __prepare__ __qualname__ __repr__ __ror__ __sizeof__ '
    '__subclasscheck__ __subclasses__ __text_signature__ __type_params__ '
    '__weakrefoffset__',
    'int': '__abs__ __add__ __and__ __bool__ __ceil__ __divmod__ __float__ '
    '__floor__ __floordiv__ __format__ __getattribute__ __getnewargs__ '
    '__index__ __int__ __invert__ __lshift__ __mod__ __mul__ __neg__ __or__ '
    '__pos__ __pow__ __radd__ __rand__ __rdivmod__ __rfloordiv__ __rlshift__ '
    '__rmod__ __rmul__ __ror__ __round__ __rpow__ __rrshift__ __rshift__ '
    '__rsub__ __rtruediv__ __rxor__ __sizeof__ __sub__ __truediv__ __trunc__ '
    '__xor__ denominator imag numerator real',
    'bool': '__and__ __invert__ __or__ __rand__ __repr__ __ror__ __rxor__ __xor__',
    'Match': '__class_getitem__ __module__ endpos lastgroup lastindex pos re regs string',
    'float': '__abs__ __add__ __bool__ __ceil__ __divmod__ __float__ __floor__ '
    '__floordiv__ __format__ __getattribute__ __getformat__ __getnewargs__ '
    '__int__ __mod__ __mul__ __neg__ __pos__ __pow__ __radd__ __rdivmod__ '
    '__rfloordiv__ __rmod__ __rmul__ __round__ __rpow__ __rsub__ '
    '__rtruediv__ __sub__ __truediv__ __trunc__ imag real',
    'complex': '__abs__ __add__ __bool__ __eq__ __format__ __getattribute__ '
    '__getnewargs__ __hash__ __mul__ __ne__ __neg__ __pos__ __pow__ __radd__ '
    '__repr__ __rmul__ __rpow__ __rsub__ __rtruediv__ __sub__ __truediv__ '
    'imag real',
    'str': '__add__ __format__ __getattribute__ __getnewargs__ __mod__ __mul__ '
    '__rmod__ __rmul__ __sizeof__',
    'bytes': '__add__ __buffer__ __bytes__ __getattribute__ __getnewargs__ '
    '__mod__ __mul__ __rmod__ __rmul__',
    'tuple': '__add__ __class_getitem__ __getattribute__ __getnewargs__ '
    '__mul__ __rmul__',
    'list': '__add__ __class_getitem__ __getattribute__ __iadd__ __imul__ '
    '__mul__ __reversed__ __rmul__ __sizeof__',
    'dict': '__class_getitem__ __getattribute__ __ge__ __gt__ __ior__ __le__ '
    '__lt__ __or__ __reversed__ __ror__ __sizeof__',
    'set': '__and__ __class_getitem__ __ge__ __getattribute__ __gt__ __iand__ '
    '__ior__ __isub__ __ixor__ __le__ __lt__ __or__ __rand__ __reduce__ '
    '__ror__ __rsub__ __rxor__ __sizeof__ __sub__ __xor__',
    'frozenset': '__and__ __class_getitem__ __contains__ __ge__ '
    '__getattribute__ __gt__ __iter__ __le__ __len__ __lt__ __or__ __rand__ '
    '__reduce__ __ror__ __rsub__ __rxor__ __sizeof__ __sub__ __xor__',
    'range': '__bool__ __contains__ __eq__ __getitem__ __hash__ __iter__ '
    '__len__ __ne__ __reduce__ __repr__ __reversed__ count index start step '
    'stop',
    'slice': '__eq__ __hash__ __reduce__ __repr__ indices start step stop',
    'NoneType': '__bool__ __repr__',
    'function': '__annotate__ __annotations__ __builtins__ __call__ '
    '__closure__ __code__ __defaults__ __dict__ __get__ __globals__ '
    '__kwdefaults__ __module__ __name__ __qualname__ __repr__ __type_params__',
    'method': '__call__ __func__ __get__ __reduce__ __self__',
    'generator': '__class_getitem__ __del__ __name__ __qualname__ __repr__ '
    '__sizeof__ gi_code gi_frame gi_running gi_suspended gi_yieldfrom',
    'coroutine': '__await__ __class_getitem__ __del__ __name__ __qualname__ '
    '__repr__ __sizeof__ close cr_await cr_code cr_frame cr_origin '
    'cr_running cr_suspended send throw',
    'module': '__dict__ __getattribute__ __repr__ __setattr__ __delattr__',
    'BaseException': '__cause__ __context__ __dict__ __reduce__ __setstate__ '
    '__suppress_context__ __traceback__ args',
    'OSError': 'characters_written errno filename filename2 strerror',
    'StopIteration': 'value',
}


def _bx_call(cls, args, kw):
    # `cls(*args, **kw)` for a class extending a builtin type.
    base = _bx_base(cls)
    init = _bx_init(cls)
    if init is not None and (base is list or base is dict or base is set):
        obj = _bx_make(cls, base())
    elif kw is None:
        obj = _bx_make(cls, base(*args))
    else:
        obj = _bx_make(cls, base(*args, **kw))
    if init is not None:
        r = init(obj, *args) if kw is None else init(obj, *args, **kw)
        if r is not None:
            raise TypeError("__init__() should return None, not '" + _type_name(r) + "'")
    return obj


def _list_extend(a):
    # CPython's `list_extend`: an iterator's items appended as it yields
    # them, a chunk per loop step (`_more`), so code it runs sees the list
    # grow and an error keeps the items before it; the list itself as the
    # source is read as it was.
    l = _bx_value(a[0])
    src = a[1]
    if src is a[0]:
        src = list(l)
    it = iter(src)
    while _more(l, it):
        pass


def _str_join(a):
    return a[0].join(_list((a[1],)))


def _sum(a):
    return _sum_rest(a[1] if len(a) > 1 else 0, _MISSING, iter(a[0]), 0, 0.0)


def _any(a):
    for x in a[0]:
        if x:
            return True
    return False


def _all(a):
    for x in a[0]:
        if not x:
            return False
    return True


def _any_rest(x, it):
    # `any` continued from a native scan: `x` is the item it stopped at
    # (False when `next` needs Python code).
    if x:
        return True
    for y in it:
        if y:
            return True
    return False


def _all_rest(x, it):
    if not x:
        return False
    for y in it:
        if not y:
            return False
    return True


def _minmax_rest(best, x, it, gt):
    # `min`/`max` continued from a native fold (`builtins::fold_run`) once
    # an item compares in Python: `x` is the item it stopped at, or
    # `_MISSING` when `next` needs Python code.
    if x is _MISSING:
        x = next(it, _MISSING)
        if x is _MISSING:
            return best
    while True:
        if (x > best) if gt else (x < best):
            best = x
        x = next(it, _MISSING)
        if x is _MISSING:
            return best


def _sum_rest(total, x, it, stage, lo):
    # CPython's sum (`builtin_sum_impl`), each item added as it is pulled,
    # from `stage` (`builtins::stage`): 0 while the total and every item
    # are ints that fit i64, 1 floats with Neumaier compensation (`lo`), 2
    # complex numbers the same way, 3 anything with `+`. A stage ends at
    # the first item it does not take, added with `+`; the next stage runs
    # only if the total then has its type. `x` is an item a native fold
    # stopped at, added with `+`, or `_MISSING`.
    if x is not _MISSING:
        total = total + x
        stage += 1
        lo = 0j if stage == 2 else 0.0
    if stage == 0:
        stage = 1
        if type(total) is int and -9223372036854775808 <= total <= 9223372036854775807:
            # One shift and a test per item (the bench Ir rule; a call such
            # as `type(s)` costs twice the add): while the total keeps its
            # sign, `s >> 63` is 0 (or -1) exactly while `s` fits i64, and
            # raises TypeError for a float or complex total. Anything else
            # stops at the check below, which also sees an item outside i64
            # (the total then changes sign). The stage ends with `s`, the
            # first sum it does not take, as CPython's does.
            s = total
            try:
                while True:
                    if total >= 0:
                        for x in it:
                            s = total + x
                            if s >> 63:
                                break
                            total = s
                        else:
                            return total
                    else:
                        for x in it:
                            s = total + x
                            if s >> 63 != -1:
                                break
                            total = s
                        else:
                            return total
                    if (type(s) is not int or type(x) is not int and type(x) is not bool
                            or not -9223372036854775808 <= s <= 9223372036854775807
                            or not -9223372036854775808 <= x <= 9223372036854775807):
                        break
                    total = s
            except TypeError:
                if s is total:
                    raise
            total = s
    if stage == 1:
        stage = 2
        if type(total) is float:
            hi = total
            for x in it:
                t = type(x)
                if t is float:
                    f = x
                elif t is int or t is bool:
                    f = float(x)
                else:
                    total = (hi + lo if lo != 0.0 and lo - lo == 0.0 else hi) + x
                    break
                t = hi + f
                if abs(hi) >= abs(f):
                    lo += (hi - t) + f
                else:
                    lo += (f - t) + hi
                hi = t
            else:
                return hi + lo if lo != 0.0 and lo - lo == 0.0 else hi
        lo = 0j
    if stage == 2 and type(total) is complex:
        stage = 3
        re, im = total.real, total.imag
        rlo, ilo = lo.real, lo.imag
        for x in it:
            t = type(x)
            if t is complex:
                f = x.real
                g = x.imag
                t = im + g
                if abs(im) >= abs(g):
                    ilo += (im - t) + g
                else:
                    ilo += (g - t) + im
                im = t
            elif t is int or t is bool:
                f = float(x)
            elif t is float:
                f = x
            else:
                total = complex(re + rlo if rlo != 0.0 and rlo - rlo == 0.0 else re,
                                im + ilo if ilo != 0.0 and ilo - ilo == 0.0 else im) + x
                break
            t = re + f
            if abs(re) >= abs(f):
                rlo += (re - t) + f
            else:
                rlo += (f - t) + re
            re = t
        else:
            return complex(re + rlo if rlo != 0.0 and rlo - rlo == 0.0 else re,
                           im + ilo if ilo != 0.0 and ilo - ilo == 0.0 else im)
    for x in it:
        total = total + x
    return total


def _minmax(a, key, default, name, gt):
    if len(a) == 1:
        it = a[0]
    else:
        if default is not _MISSING:
            raise TypeError(f'Cannot specify a default for {name}() with multiple positional arguments')
        it = a
    if key is _MISSING:
        key = None
    best = _MISSING
    bestk = None
    for x in it:
        k = x if key is None else key(x)
        if best is _MISSING:
            best = x
            bestk = k
        elif gt:
            if k > bestk:
                best = x
                bestk = k
        elif k < bestk:
            best = x
            bestk = k
    if best is _MISSING:
        if default is not _MISSING:
            return default
        raise ValueError(f'{name}() iterable argument is empty')
    return best


def _min(a, key, default):
    return _minmax(a, key, default, 'min', False)


def _max(a, key, default):
    return _minmax(a, key, default, 'max', True)


def _sort(xs, key, reverse):
    if key is _MISSING or key is None:
        keys = list(xs)
    else:
        keys = [key(x) for x in xs]
    return _sort_keys(xs, keys, reverse is not _MISSING and reverse)


def _sort_keys(xs, keys, rev):
    n = len(xs)
    idx = list(range(n))
    if rev:
        idx.reverse()
    buf = list(idx)
    width = 1
    while width < n:
        i = 0
        while i < n:
            mid = i + width
            if mid > n:
                mid = n
            hi = i + 2 * width
            if hi > n:
                hi = n
            l = i
            r = mid
            k = i
            while l < mid and r < hi:
                if keys[idx[r]] < keys[idx[l]]:
                    buf[k] = idx[r]
                    r += 1
                else:
                    buf[k] = idx[l]
                    l += 1
                k += 1
            while l < mid:
                buf[k] = idx[l]
                l += 1
                k += 1
            while r < hi:
                buf[k] = idx[r]
                r += 1
                k += 1
            i += 2 * width
        idx, buf = buf, idx
        width *= 2
    if rev:
        idx.reverse()
    return [xs[i] for i in idx]


def _gc_wait():
    # A native step's wait for the collection it asked for
    # (`Vm::oom_defer`): this call's safepoint runs it to its end.
    return None


def _sort_rest(xs, keys, rev, st):
    # A long sort merges a slice per loop step, each a safepoint
    # (docs/spec.md §5); keys that need Python to compare sort here.
    while True:
        r = _more(xs, keys, st)
        if r is not None:
            break
    if r is False:
        return _sort_keys(xs, keys, rev)
    return r


def _sorted(a, key, reverse):
    return _sort(_list(a), key, reverse)


def _list_sort(a, key, reverse):
    # CPython detaches the items while it sorts: callbacks see an empty
    # list, and refilling it is an error once the sorted items are back.
    l = a[0]
    items = list(l)
    _list_replace(l, [])
    try:
        items = _sort(items, key, reverse)
    finally:
        modified = len(l) != 0
        _list_replace(l, items)
    if modified:
        raise ValueError('list modified during sort')


def _enum_gen(it, n):
    for x in it:
        yield (n, x)
        n += 1


def _enumerate(a):
    # Here when the start has `__index__` or the iterable is no builtin
    # iterator. The start is read first, as CPython's PyNumber_Index; the
    # native enumerate then makes any int (a bool, a subclass, one past
    # 2**63) an exact int, or raises CPython's TypeError, at the call.
    if len(a) < 2:
        return _enum_gen(iter(a[0]), 0)
    n = a[1]
    if not isinstance(n, int) and _has_index(n):
        n = _index(n)
    n = next(enumerate((0,), n))[0]
    return _enum_gen(iter(a[0]), n)


def _zip_gen(iters, strict, name='zip'):
    if not iters:
        return
    while True:
        out = []
        i = 0
        for it in iters:
            try:
                out.append(next(it))
            except StopIteration:
                if strict:
                    if i:
                        raise ValueError(f'{name}() argument {i + 1} is shorter than argument{"s 1-" if i > 1 else " "}{i}') from None
                    for k in range(1, len(iters)):
                        try:
                            next(iters[k])
                        except StopIteration:
                            continue
                        raise ValueError(f'{name}() argument {k + 1} is longer than argument{"s 1-" if k > 1 else " "}{k}') from None
                return
            i += 1
        yield tuple(out)


def _zip(a, strict):
    return _zip_gen([iter(x) for x in a], strict is not _MISSING and strict)


def _map_gen(f, iters):
    if len(iters) == 1:
        for x in iters[0]:
            yield f(x)
        return
    while True:
        args = []
        for it in iters:
            try:
                args.append(next(it))
            except StopIteration:
                return
        yield f(*args)


def _map_strict(f, iters):
    for args in _zip_gen(iters, True, 'map'):
        yield f(*args)


def map(*args, strict=False):
    if len(args) < 2:
        raise TypeError('map() must have at least two arguments.')
    iters = [iter(i) for i in args[1:]]
    if strict:
        return _map_strict(args[0], iters)
    return _map_gen(args[0], iters)


def _filter_gen(f, it):
    if f is None:
        for x in it:
            if x:
                yield x
    else:
        for x in it:
            if f(x):
                yield x


def filter(f, iterable, /):
    return _filter_gen(f, iter(iterable))


def open(file, mode='r', buffering=-1, encoding=None, errors=None, newline=None, closefd=True,
         opener=None):
    # The sandbox has no filesystem: an embedder that has one registers an
    # `open` host function, which replaces this builtin (docs/spec.md §10).
    if not isinstance(file, (str, bytes, int)) and not hasattr(type(file), '__fspath__'):
        raise TypeError('expected str, bytes or os.PathLike object, not '
                        + type(file).__name__)
    if not isinstance(mode, str):
        raise TypeError("open() argument 'mode' must be str, not " + type(mode).__name__)
    seen = ''
    for c in mode:
        if c not in 'rwxabt+' or c in seen:
            raise ValueError('invalid mode: ' + repr(mode))
        seen += c
    kinds = ('r' in seen) + ('w' in seen) + ('x' in seen) + ('a' in seen)
    if kinds != 1:
        raise ValueError('must have exactly one of create/read/write/append mode')
    if 'b' in seen and 't' in seen:
        raise ValueError("can't have text and binary mode at once")
    import _mm
    hint = _mm.hint('os')
    raise PermissionError("open() is not available: this sandbox has no ambient filesystem"
                          + ('. ' + hint if hint else ''))


def _str_format(a, kw):
    # CPython's MarkupIterator (Objects/stringlib/unicode_format.h).
    if kw is _MISSING:
        kw = None
    return _fmt_build(a[0], a[1:], kw, [0, 0], 2)


def _str_format_map(a):
    return _fmt_build(a[0], None, a[1], [0, 0], 2)


def _str_maketrans(a):
    # CPython's unicode_maketrans; `a[0]` is the type.
    if len(a) == 2:
        x = a[1]
        if not isinstance(x, dict):
            raise TypeError('if you give only one argument to maketrans it must be a dict')
        new = {}
        for k, v in x.items():
            if isinstance(k, str):
                if len(k) != 1:
                    # CPython's messages, missing spaces included.
                    raise ValueError('string keys in translatetable must be of length 1')
                new[ord(k)] = v
            elif isinstance(k, int):
                new[k] = v
            else:
                raise TypeError('keys in translate table mustbe strings or integers')
        return new
    x, y = a[1], a[2]
    for i in range(2, len(a)):
        if not isinstance(a[i], str):
            raise TypeError(f'maketrans() argument {i} must be str, not {type(a[i]).__name__}')
    if not isinstance(x, str):
        raise TypeError('first maketrans argument must be a string if there is a second argument')
    if len(x) != len(y):
        raise ValueError('the first two maketrans arguments must have equal length')
    new = {}
    for i in range(len(x)):
        new[ord(x[i])] = ord(y[i])
    if len(a) > 3:
        for c in a[3]:
            new[ord(c)] = None
    return new


def _str_translate(a):
    s, table = a
    out = []
    for c in s:
        try:
            v = table[ord(c)]
        except LookupError:
            out.append(c)
            continue
        if v is None:
            continue
        if isinstance(v, int):
            if not 0 <= v < 0x110000:
                raise ValueError('character mapping must be in range(0x110000)')
            out.append(chr(v))
        elif isinstance(v, str):
            out.append(v)
        else:
            raise TypeError('character mapping must return integer, None or str')
    return ''.join(out)


def _str_split_long(s, sep, n, right):
    # `split` (`rsplit` when `right`) by a separator of 512 KiB or more,
    # at most `n` times (-1: all): each `find` keeps the separator's
    # factorization between its steps (strmeth.rs `long_find`); at most
    # `len(s) // len(sep)` matches.
    out = []
    k = len(sep)
    if right:
        j = len(s)
        while n:
            i = s.rfind(sep, 0, j)
            if i < 0:
                break
            out.append(s[i + k:j])
            j = i
            n -= 1
        out.append(s[:j])
        out.reverse()
        return out
    i = 0
    while n:
        j = s.find(sep, i)
        if j < 0:
            break
        out.append(s[i:j])
        i = j + k
        n -= 1
    out.append(s[i:])
    return out


def _str_replace_long(s, old, new, n):
    return new.join(_str_split_long(s, old, n, False))


def _str_partition(s, sep, arg, right, this):
    # `partition` (`rpartition`) of a long str: its search goes a step at
    # a time; a found separator is the argument itself, and with none the
    # str is `this`, the object called on (a subclass instance too).
    i = s.rfind(sep) if right else s.find(sep)
    if i < 0:
        return ('', '', this) if right else (this, '', '')
    return (s[:i], arg, s[i + len(sep):])


def _float_fromhex(a):
    return a[0](float.fromhex(a[1]))


def _bytes_fromhex(a):
    return a[0](bytes.fromhex(a[1]))


def _int_from_bytes(a, signed):
    # An object with `__bytes__`, as CPython's `PyObject_Bytes` reads it,
    # or items with `__index__`, as `bytes()` reads them; `a[0]` is the
    # class, maybe a subclass.
    b = a[1]
    f = getattr(type(b), '__bytes__', None)
    if f is None:
        b = bytes(b)
    else:
        b = f(b)
        if not isinstance(b, bytes):
            raise TypeError('__bytes__ returned non-bytes (type ' + _type_name(b) + ')')
    return a[0].from_bytes(b, *a[2:], signed=False if signed is _MISSING else signed)


def _fmt_int(s):
    # `get_integer`: -1 unless every character is a decimal digit.
    if not s or not s.isdecimal():
        return -1
    return int(s)


def _fmt_build(fmt, args, kw, auto, depth):
    if depth <= 0:
        raise ValueError('Max string recursion exceeded')
    out = []
    r = None
    i = 0
    n = len(fmt)
    # The next '{' and '}' at or after `i` (-1: none left), found natively
    # and kept until passed, so a long literal run is one search.
    lb = rb = -2
    while i < n:
        start = i
        if lb != -1 and lb < i:
            lb = fmt.find('{', i)
        if rb != -1 and rb < i:
            rb = fmt.find('}', i)
        if lb < 0 and rb < 0:
            out.append(fmt[start:])
            break
        i = (rb if lb < 0 else lb if rb < 0 else min(lb, rb)) + 1
        c = fmt[i - 1]
        if c == '}' and (i >= n or fmt[i] != '}'):
            raise ValueError("Single '}' encountered in format string")
        if i >= n and c == '{':
            raise ValueError("Single '{' encountered in format string")
        if fmt[i] == c:
            out.append(fmt[start:i])
            i += 1
            continue
        out.append(fmt[start:i - 1])
        # parse_field
        fstart = i
        c = ''
        while i < n:
            c = fmt[i]
            i += 1
            if c == '{':
                raise ValueError("unexpected '{' in field name")
            if c == '[':
                while i < n and fmt[i] != ']':
                    i += 1
                continue
            if c == '}' or c == ':' or c == '!':
                break
        name = fmt[fstart:i - 1] if c in '}:!' and c else fmt[fstart:i]
        conv = ''
        spec = ''
        expand = False
        if c == '!' or c == ':':
            if c == '!':
                if i >= n:
                    raise ValueError('end of string while looking for conversion specifier')
                conv = fmt[i]
                i += 1
                c = ''
                if i < n:
                    c = fmt[i]
                    i += 1
                    if c != '}' and c != ':':
                        raise ValueError("expected ':' after conversion specifier")
            if c != '}':
                sstart = i
                count = 1
                while i < n:
                    c = fmt[i]
                    i += 1
                    if c == '{':
                        expand = True
                        count += 1
                    elif c == '}':
                        count -= 1
                        if count == 0:
                            break
                if count:
                    raise ValueError("unmatched '{' in format spec")
                spec = fmt[sstart:i - 1]
        elif c != '}' or i > n:
            raise ValueError("expected '}' before end of string")
        v = _fmt_field(name, args, kw, auto)
        if conv and conv != '\x00':
            # A NUL conversion is no conversion, as in CPython.
            if conv == 'r':
                v = repr(v)
            elif conv == 's':
                v = str(v)
            elif conv == 'a':
                v = ascii(v)
            elif ' ' < conv < '\x7f':
                raise ValueError(f'Unknown conversion specifier {conv}')
            else:
                raise ValueError(f'Unknown conversion specifier \\x{ord(conv):x}')
        if expand:
            spec = _fmt_build(spec, args, kw, auto, depth - 1)
            # A formatter gets the expanded spec as an exact str, as
            # CPython's `render_field` copies it.
            if type(spec) is not str:
                spec = str.__str__(spec)
        r = format(v, spec)
        if i >= n and r and not any(out):
            # As CPython's writer: a non-empty field that ends the
            # template, after nothing but empty text, is the result as
            # `format` gave it (a str subclass from `__format__` too).
            return r
        out.append(r)
    return ''.join(out)


def _fmt_field(name, args, kw, auto):
    k = 0
    n = len(name)
    while k < n and name[k] != '.' and name[k] != '[':
        k += 1
    first = name[:k]
    idx = _fmt_int(first)
    empty = not first
    if empty or idx != -1:
        if auto[0] == 0:
            auto[0] = 1 if empty else 2
        if auto[0] == 2 and empty:
            raise ValueError('cannot switch from manual field specification to automatic field numbering')
        if auto[0] == 1 and not empty:
            raise ValueError('cannot switch from automatic field numbering to manual field specification')
        if empty:
            idx = auto[1]
            auto[1] += 1
    if idx == -1:
        if kw is None:
            raise KeyError(first)
        v = kw[first]
    else:
        if args is None:
            raise ValueError('Format string contains positional fields')
        if idx >= len(args):
            raise IndexError(f'Replacement index {idx} out of range for positional args tuple')
        v = args[idx]
    while k < n:
        c = name[k]
        k += 1
        if c == '.':
            s = k
            while k < n and name[k] != '.' and name[k] != '[':
                k += 1
            part = name[s:k]
            if not part:
                raise ValueError('Empty attribute in format string')
            v = getattr(v, part)
        elif c == '[':
            s = k
            while k < n and name[k] != ']':
                k += 1
            if k >= n:
                raise ValueError("Missing ']' in format string")
            part = name[s:k]
            k += 1
            if not part:
                raise ValueError('Empty attribute in format string')
            j = _fmt_int(part)
            v = v[part] if j == -1 else v[j]
        else:
            raise ValueError("Only '.' or '[' may follow ']' in format field specifier")
    return v


def _pow_slot(t):
    # Which `nb_power` type `t` has: user classes defining `__pow__` or
    # `__rpow__` share one slot (CPython's `slot_nb_power`).
    for k in t.__mro__:
        if k is int:
            return 'i'
        if k is float:
            return 'f'
        if k is complex:
            return 'c'
        d = getattr(k, '__dict__', None)
        if d is not None and ('__pow__' in d or '__rpow__' in d):
            return 'u'
    return None


def _pow_call(slot, v, w, z):
    if slot == 'i':
        if isinstance(v, int) and isinstance(w, int) and isinstance(z, int):
            return pow(_int_value(v), _int_value(w), _int_value(z))
        return NotImplemented
    if slot == 'f':
        raise TypeError('pow() 3rd argument not allowed unless all arguments are integers')
    if slot == 'c':
        if isinstance(v, (int, float, complex)) and isinstance(w, (int, float, complex)):
            raise ValueError('complex modulo')
        return NotImplemented
    # `slot_nb_power` with a modulus (3.14 tries `__rpow__` too).
    return _slot_full(v, w, '__pow__', '__rpow__', _pow_slot, z)


def _slot_full(v, w, name, rname, slot, *z):
    # CPython's SLOT1BINFULL: the left operand's method, the right's
    # reflected one (first if its type is a subclass that overrides it).
    tv, tw = type(v), type(w)
    other = tv is not tw and slot(tw) == 'u'
    if slot(tv) == 'u':
        if other and issubclass(tw, tv) and getattr(tw, rname, None) is not getattr(tv, rname, None):
            r = _pow_maybe(w, rname, v, *z)
            if r is not NotImplemented:
                return r
            other = False
        r = _pow_maybe(v, name, w, *z)
        if r is not NotImplemented or tw is tv:
            return r
    if other:
        return _pow_maybe(w, rname, v, *z)
    return NotImplemented


def _divmod_slot(t):
    for k in t.__mro__:
        if k is int or k is float:
            return 'b'
        d = getattr(k, '__dict__', None)
        if d is not None and ('__divmod__' in d or '__rdivmod__' in d):
            return 'u'
    return None


def _divmod_call(slot, v, w):
    if slot == 'b':
        if isinstance(v, (int, float)) and isinstance(w, (int, float)):
            return divmod(float(v) if isinstance(v, float) else _int_value(v),
                          float(w) if isinstance(w, float) else _int_value(w))
        return NotImplemented
    return _slot_full(v, w, '__divmod__', '__rdivmod__', _divmod_slot)


def _divmod(a):
    # `binary_op1` for divmod() with user operands.
    v, w = a
    tv, tw = type(v), type(w)
    sv = _divmod_slot(tv)
    sw = _divmod_slot(tw) if tw is not tv else None
    if sw == sv and sv == 'u':
        sw = None
    if sv is not None:
        if sw is not None and issubclass(tw, tv):
            r = _divmod_call(sw, v, w)
            if r is not NotImplemented:
                return r
            sw = None
        r = _divmod_call(sv, v, w)
        if r is not NotImplemented:
            return r
    if sw is not None:
        r = _divmod_call(sw, v, w)
        if r is not NotImplemented:
            return r
    raise TypeError(f"unsupported operand type(s) for divmod(): '{type(v).__name__}' and '{type(w).__name__}'")


def _pow_maybe(obj, name, *a):
    f = getattr(type(obj), name, _MISSING)
    if f is _MISSING:
        return NotImplemented
    return f(obj, *a)


def _pow(a):
    if len(a) < 3 or a[2] is None:
        return a[0] ** a[1]
    v, w, z = a
    sv = _pow_slot(type(v))
    sw = _pow_slot(type(w)) if type(w) is not type(v) else None
    if sw == sv:
        sw = None
    if sv is not None:
        if sw is not None and issubclass(type(w), type(v)):
            r = _pow_call(sw, v, w, z)
            if r is not NotImplemented:
                return r
            sw = None
        r = _pow_call(sv, v, w, z)
        if r is not NotImplemented:
            return r
    if sw is not None:
        r = _pow_call(sw, v, w, z)
        if r is not NotImplemented:
            return r
    sz = _pow_slot(type(z))
    if sz is not None and sz != sv and sz != sw:
        r = _pow_call(sz, v, w, z)
        if r is not NotImplemented:
            return r
    raise TypeError(f"unsupported operand type(s) for ** or pow(): '{type(v).__name__}', "
                    f"'{type(w).__name__}', '{type(z).__name__}'")


# ------------------------------------------------------------- classes


def _build_bases(name, bases, ns, kw, cell):
    # PEP 560: a base that is not a class stands for what its
    # `__mro_entries__(bases)` returns; the class keeps the originals.
    new = []
    changed = False
    for b in bases:
        if not isinstance(b, type):
            entries = getattr(b, '__mro_entries__', None)
            if entries is not None:
                r = entries(bases)
                if not isinstance(r, tuple):
                    raise TypeError('__mro_entries__ must return a tuple')
                new.extend(r)
                changed = True
                continue
        new.append(b)
    if changed:
        ns['__orig_bases__'] = bases
    return _build_class(name, tuple(new), ns, kw, cell)


def _finish_class(cls, kwargs):
    for name, value in list(cls.__dict__.items()):
        set_name = getattr(type(value), '__set_name__', None)
        if set_name is not None:
            set_name(value, cls, name)
    super(cls, cls).__init_subclass__(**kwargs)
    return cls


# --------------------------------------------------------------- match

_MATCH_SELF = (bool, bytes, dict, float, frozenset, int, list, set, str, tuple)


def _op_match(kind, x, a, b, n):
    if kind == 2:
        return len(x)
    if kind == 3:
        get = a.get
        seen = set()
        vals = []
        for k in x:
            if k in seen:
                raise ValueError(f'mapping pattern checks duplicate key ({k!r})')
            seen.add(k)
            v = get(k, _MISSING)
            if v is _MISSING:
                return None
            vals.append(v)
        return tuple(vals)
    if kind == 4:
        d = dict(a)
        for k in x:
            del d[k]
        return d
    return _match_class(x, a, b, n)


def _match_attr(x, name, a, seen, attrs):
    if a in seen:
        raise TypeError(f'{name}() got multiple sub-patterns for attribute {a!r}')
    seen.append(a)
    v = getattr(x, a, _MISSING)
    if v is _MISSING:
        return False
    attrs.append(v)
    return True


def _match_class(x, cls, kw, nargs):
    if not isinstance(cls, type):
        raise TypeError('called match pattern must be a class')
    if not isinstance(x, cls):
        return None
    name = cls.__name__
    attrs = []
    seen = []
    if nargs:
        ma = getattr(cls, '__match_args__', _MISSING)
        match_self = False
        if ma is _MISSING:
            ma = ()
            match_self = issubclass(cls, _MATCH_SELF)
        elif type(ma) is not tuple:
            raise TypeError(f'{name}.__match_args__ must be a tuple (got {type(ma).__name__})')
        allowed = 1 if match_self else len(ma)
        if allowed < nargs:
            s = '' if allowed == 1 else 's'
            raise TypeError(f'{name}() accepts {allowed} positional sub-pattern{s} ({nargs} given)')
        if match_self:
            attrs.append(x)
        else:
            for i in range(nargs):
                a = ma[i]
                if type(a) is not str:
                    raise TypeError(f'__match_args__ elements must be strings (got {type(a).__name__})')
                if not _match_attr(x, name, a, seen, attrs):
                    return None
    for a in kw:
        if not _match_attr(x, name, a, seen, attrs):
            return None
    return tuple(attrs)


def help(*args, **kwds):
    # The stub of a host function, class or module (the host's signature
    # and docstring), else the object's name and docstring. CPython's
    # pydoc pages are not reproduced.
    import _mm
    if not args:
        print("Call help(obj) on a function, class or module to see its signature "
              "and documentation.")
        return
    obj = args[0]
    text = _mm.host_help(obj)
    if text is None:
        name = getattr(obj, '__qualname__', None) or getattr(obj, '__name__', None)
        if not isinstance(name, str):
            name = type(obj).__name__
        text = f"Help on {type(obj).__name__} {name}:\n\n{name}\n"
        doc = getattr(obj, '__doc__', None)
        if isinstance(doc, str) and doc:
            text += '    ' + doc.replace('\n', '\n    ') + '\n'
    print(text, end='' if text.endswith('\n') else '\n')


# ---------------------------------------------------------- reporting


def _post_complete(v):
    return (repr(v), str(v))


def _post_render(e):
    # str() of every exception the report shows: the chain, and the
    # sub-exceptions of exception groups (as deep as the report draws).
    seen = []
    todo = [(e, 0)]
    while todo:
        x, depth = todo.pop()
        while x is not None:
            for y in seen:
                if y is x:
                    x = None
                    break
            if x is None:
                break
            seen.append(x)
            try:
                s = str(x)
            except BaseException:
                s = None
            if type(s) is str:
                _set_exc_str(x, s)
            if depth <= 10 and isinstance(x, BaseExceptionGroup):
                for sub in x._mm_excs[:16]:
                    todo.append((sub, depth + 1))
            c = x.__cause__
            if c is None and not x.__suppress_context__:
                c = x.__context__
            x = c


# Future and Task reprs: host futures exist without `asyncio`.


def _aio_repr(f):
    # base_futures._future_repr_info and base_tasks._task_repr_info,
    # without coro=, wait_for= and cb= (no code objects here).
    if f.cancelled():
        state = 'cancelled'
    elif f.done():
        state = 'finished'
    elif type(f) is Task and f.cancelling():
        state = 'cancelling'
    else:
        state = 'pending'
    info = [state]
    if type(f) is Task:
        info.append('name=' + repr(f.get_name()))
    if state == 'finished':
        exc = f.exception()
        if exc is not None:
            info.append('exception=' + repr(exc))
        else:
            info.append('result=' + _rl_repr(f.result(), 6))
    return '<' + ('Task ' if type(f) is Task else 'Future ') + ' '.join(info) + '>'


def _rl_cut(s, n):
    if len(s) > n:
        i = max(0, (n - 3) // 2)
        j = max(0, n - 3 - i)
        s = s[:i] + '...' + s[len(s) - j:]
    return s


def _rl_sorted(x):
    try:
        return sorted(x)
    except Exception:
        return list(x)


def _rl_repr(x, level):
    # reprlib.repr with reprlib.Repr's default limits.
    t = type(x)
    if t is str:
        s = repr(x[:30])
        if len(s) > 30:
            i = max(0, 27 // 2)
            j = max(0, 27 - i)
            s = repr(x[:i] + x[len(x) - j:])
            s = s[:i] + '...' + s[len(s) - j:]
        return s
    if t is int:
        return _rl_cut(repr(x), 40)
    if t is list or t is tuple or t is set or t is frozenset:
        n = len(x)
        if t is set and not n:
            return 'set()'
        if t is frozenset and not n:
            return 'frozenset()'
        left, right = {list: ('[', ']'), tuple: ('(', ')'), set: ('{', '}'),
                       frozenset: ('frozenset({', '})')}[t]
        if level <= 0 and n:
            body = '...'
        else:
            items = _rl_sorted(x) if t is set or t is frozenset else x
            pieces = [_rl_repr(e, level - 1) for e in items[:6]]
            if n > 6:
                pieces.append('...')
            body = ', '.join(pieces)
            if n == 1 and t is tuple:
                right = ',)'
        return left + body + right
    if t is dict:
        n = len(x)
        if n == 0:
            return '{}'
        if level <= 0:
            return '{...}'
        pieces = [_rl_repr(k, level - 1) + ': ' + _rl_repr(x[k], level - 1)
                  for k in _rl_sorted(x)[:4]]
        if n > 4:
            pieces.append('...')
        return '{' + ', '.join(pieces) + '}'
    try:
        s = repr(x)
    except Exception:
        return f'<{t.__name__} instance at {id(x):#x}>'
    return _rl_cut(s, 30)


# ---------------------------------------------------------------- json
# The `json` module (docs/spec.md §12): argument handling as in CPython's
# json/__init__.py, encoder.py and decoder.py; encoding and scanning are
# native, and anything the native encoder cannot write itself (a `default=`
# call, an error with its `__notes__`) is encoded again by `_json_py`,
# which follows CPython's C encoder.


__name__ = 'json.decoder'


class JSONDecodeError(ValueError):

    def __init__(self, msg, doc, pos):
        lineno = doc.count('\n', 0, pos) + 1
        colno = pos - doc.rfind('\n', 0, pos)
        ValueError.__init__(self, f'{msg}: line {lineno} column {colno} (char {pos})')
        self.msg = msg
        self.doc = doc
        self.pos = pos
        self.lineno = lineno
        self.colno = colno


def _json_opt(qual, args, kwargs, names, defaults, npos):
    # Optional parameters of a prelude method (class units take none); the
    # first `npos` may be positional.
    if len(args) > npos:
        raise TypeError(f'{qual}() takes {npos + 1} positional argument{"s" if npos else ""} '
                        f'but {len(args) + 1} were given')
    out = list(args) + list(defaults[len(args):])
    for k in kwargs:
        if k not in names:
            raise TypeError(f"{qual}() got an unexpected keyword argument '{k}'")
        i = names.index(k)
        if i < len(args):
            raise TypeError(f"{qual}() got multiple values for argument '{k}'")
        out[i] = kwargs[k]
    return out


class JSONDecoder:

    def __init__(self, *args, **kwargs):
        oh, pf, pi, pc, strict, ph = _json_opt(
            'JSONDecoder.__init__', args, kwargs,
            ('object_hook', 'parse_float', 'parse_int', 'parse_constant', 'strict', 'object_pairs_hook'),
            (None, None, None, None, True, None), 0)
        self.object_hook = oh
        self.parse_float = pf or float
        self.parse_int = pi or int
        self.parse_constant = pc
        self.strict = strict
        self.object_pairs_hook = ph

    def decode(self, s, *args):
        return _json_decode(self, s, None)

    def raw_decode(self, s, *args, **kwargs):
        idx, = _json_opt('JSONDecoder.raw_decode', args, kwargs, ('idx',), (0,), 1)
        if idx < 0:
            raise ValueError('idx cannot be negative')
        return _json_decode(self, s, idx)


def _json_decode(dec, s, idx):
    pf = dec.parse_float
    pi = dec.parse_int
    hooks = (dec.object_hook, None if pf is float else pf, None if pi is int else pi,
             dec.parse_constant, dec.object_pairs_hook)
    raw = hooks != (None, None, None, None, None)
    r = _json_scan(s, raw, idx, dec.strict)
    if len(r) == 2:
        raise JSONDecodeError(r[0], s, r[1]) from None
    v = _json_hooks(r[0], hooks) if raw else r[0]
    return v if idx is None else (v, r[1])


__name__ = 'json.encoder'


class JSONEncoder:
    item_separator = ', '
    key_separator = ': '

    def __init__(self, *args, **kwargs):
        skipkeys, ensure_ascii, check_circular, allow_nan, sort_keys, indent, separators, default = _json_opt(
            'JSONEncoder.__init__', args, kwargs,
            ('skipkeys', 'ensure_ascii', 'check_circular', 'allow_nan', 'sort_keys', 'indent', 'separators',
             'default'),
            (False, True, True, True, False, None, None, None), 0)
        self.skipkeys = skipkeys
        self.ensure_ascii = ensure_ascii
        self.check_circular = check_circular
        self.allow_nan = allow_nan
        self.sort_keys = sort_keys
        self.indent = indent
        if separators is not None:
            self.item_separator, self.key_separator = separators
        elif indent is not None:
            self.item_separator = ','
        if default is not None:
            self.default = default

    def default(self, o):
        raise TypeError(f'Object of type {o.__class__.__name__} is not JSON serializable')

    def encode(self, o):
        if isinstance(o, str):
            return _json_str(o, self.ensure_ascii)
        chunks = self.iterencode(o, _one_shot=True)
        if not isinstance(chunks, (list, tuple)):
            chunks = list(chunks)
        return ''.join(chunks)

    def iterencode(self, o, *args, **kwargs):
        indent = self.indent
        if indent is not None and not isinstance(indent, str):
            indent = ' ' * indent
        return [_json_dumps(o, indent, self.key_separator, self.item_separator, self.sort_keys,
                            self.skipkeys, self.allow_nan, self.ensure_ascii, self.check_circular,
                            self.default)]


__name__ = 'json'


def _json_kw(kw, what):
    for k in kw:
        raise TypeError(f"{what}.__init__() got an unexpected keyword argument '{k}'")


def dumps(obj, *, skipkeys=False, ensure_ascii=True, check_circular=True, allow_nan=True, cls=None,
          indent=None, separators=None, default=None, sort_keys=False, **kw):
    if cls is not None:
        return cls(skipkeys=skipkeys, ensure_ascii=ensure_ascii, check_circular=check_circular,
                   allow_nan=allow_nan, indent=indent, separators=separators, default=default,
                   sort_keys=sort_keys, **kw).encode(obj)
    _json_kw(kw, 'JSONEncoder')
    if separators is not None:
        item_sep, key_sep = separators
    elif indent is not None:
        item_sep, key_sep = ',', ': '
    else:
        item_sep, key_sep = ', ', ': '
    if isinstance(obj, str):
        return _json_str(obj, ensure_ascii)
    if indent is not None and not isinstance(indent, str):
        indent = ' ' * indent
    return _json_dumps(obj, indent, key_sep, item_sep, sort_keys, skipkeys, allow_nan, ensure_ascii,
                       check_circular, default)


def _json_steps(st, r):
    # A long `_json_encode` (`Vm::json_dump_step`), a step per call, each a
    # safepoint (docs/spec.md §5): `r` is `st` while steps remain, or the
    # keys of a large dict under `sort_keys`, sorted here (in steps too)
    # before the next step writes it.
    while r is st or type(r) is list:
        if r is not st:
            r.sort()
        r = _more(st)
    return r


def _json_dumps(obj, indent, key_sep, item_sep, sort_keys, skipkeys, allow_nan, ensure_ascii,
                check_circular, default):
    r = _json_encode(obj, indent, key_sep, item_sep, sort_keys, skipkeys, allow_nan, ensure_ascii,
                     check_circular, default)
    if r is _MISSING:
        out = []
        markers = {} if check_circular else None
        _json_py(obj, (markers, default, ensure_ascii, indent, key_sep, item_sep, sort_keys, skipkeys,
                       allow_nan, out), 0)
        r = ''.join(out)
    return r


def dump(obj, fp, *, skipkeys=False, ensure_ascii=True, check_circular=True, allow_nan=True, cls=None,
         indent=None, separators=None, default=None, sort_keys=False, **kw):
    # Encoding (and its keyword check) comes before `fp.write` is looked up.
    s = dumps(obj, skipkeys=skipkeys, ensure_ascii=ensure_ascii, check_circular=check_circular,
              allow_nan=allow_nan, cls=cls, indent=indent, separators=separators, default=default,
              sort_keys=sort_keys, **kw)
    fp.write(s)


def _json_mark(markers, o):
    if markers is not None:
        if id(o) in markers:
            raise ValueError('Circular reference detected')
        markers[id(o)] = o


def _json_tname(o):
    # CPython's `%T`: the type's fully qualified name.
    t = type(o)
    m = t.__module__
    if m == 'builtins' or m == '__main__':
        return t.__qualname__
    return f'{m}.{t.__qualname__}'


def _json_py(o, st, level):
    markers, default, ea, indent, ksep, isep, sort_keys, skipkeys, allow_nan, out = st
    if o is None:
        out.append('null')
    elif o is True:
        out.append('true')
    elif o is False:
        out.append('false')
    elif isinstance(o, str):
        out.append(_json_str(o, ea))
    elif isinstance(o, int):
        out.append(int.__repr__(o))
    elif isinstance(o, float):
        out.append(_json_float(o, allow_nan))
    elif isinstance(o, (list, tuple)):
        if not o:
            out.append('[]')
            return
        _json_mark(markers, o)
        out.append('[')
        sep = isep
        if indent is not None:
            level += 1
            sep = isep + '\n' + indent * level
            out.append('\n' + indent * level)
        for i, v in enumerate(o):
            if i:
                out.append(sep)
            try:
                _json_py(v, st, level)
            except BaseException as exc:
                exc.add_note(f'when serializing {_json_tname(o)} item {i}')
                raise
        if indent is not None:
            out.append('\n' + indent * (level - 1))
        out.append(']')
        if markers is not None:
            del markers[id(o)]
    elif isinstance(o, dict):
        if not o:
            out.append('{}')
            return
        _json_mark(markers, o)
        out.append('{')
        nl = None
        sep = isep
        if indent is not None:
            level += 1
            nl = '\n' + indent * level
            sep = isep + nl
        first = True
        for k, v in (sorted(o.items()) if sort_keys else o.items()):
            key = k
            if isinstance(k, str):
                pass
            elif isinstance(k, float):
                k = _json_float(k, allow_nan)
            elif k is True or k is False or k is None:
                k = 'true' if k else 'false' if k is False else 'null'
            elif isinstance(k, int):
                k = int.__repr__(k)
            elif skipkeys:
                continue
            else:
                raise TypeError(f'keys must be str, int, float, bool or None, not {type(k).__name__}')
            if first:
                first = False
                if nl is not None:
                    out.append(nl)
            else:
                out.append(sep)
            out.append(_json_str(k, ea))
            out.append(ksep)
            try:
                _json_py(v, st, level)
            except BaseException as exc:
                exc.add_note(f'when serializing {_json_tname(o)} item {key!r}')
                raise
        if not first and nl is not None:
            out.append('\n' + indent * (level - 1))
        out.append('}')
        if markers is not None:
            del markers[id(o)]
    else:
        _json_mark(markers, o)
        if default is None:
            raise TypeError(f'Object of type {o.__class__.__name__} is not JSON serializable')
        new = default(o)
        try:
            _json_py(new, st, level)
        except BaseException as exc:
            exc.add_note(f'when serializing {_json_tname(o)} object')
            raise
        if markers is not None:
            del markers[id(o)]


def loads(s, *, cls=None, object_hook=None, parse_float=None, parse_int=None, parse_constant=None,
          object_pairs_hook=None, **kw):
    if isinstance(s, str):
        if s.startswith('\ufeff'):
            raise JSONDecodeError('Unexpected UTF-8 BOM (decode using utf-8-sig)', s, 0)
    else:
        if not isinstance(s, bytes):
            raise TypeError(f'the JSON object must be str, bytes or bytearray, not {type(s).__name__}')
        if s.startswith(b'\xef\xbb\xbf'):
            s = s[3:]
        s = s.decode('utf-8')
    if (cls is None and object_hook is None and parse_int is None and parse_float is None
            and parse_constant is None and object_pairs_hook is None and not kw):
        r = _json_scan(s)
        if len(r) == 2:
            raise JSONDecodeError(r[0], s, r[1]) from None
        return r[0]
    if cls is None:
        cls = JSONDecoder
    if object_hook is not None:
        kw['object_hook'] = object_hook
    if object_pairs_hook is not None:
        kw['object_pairs_hook'] = object_pairs_hook
    if parse_float is not None:
        kw['parse_float'] = parse_float
    if parse_int is not None:
        kw['parse_int'] = parse_int
    if parse_constant is not None:
        kw['parse_constant'] = parse_constant
    return cls(**kw).decode(s)


def _json_hooks(v, hooks):
    # Finishes `_json_scan(s, True)`'s tree children-first in document order,
    # the order CPython's scanner calls the hooks in: a 1-tuple is a number
    # or constant's text, another tuple an object's keys and values.
    oh, pf, pi, pc, ph = hooks
    stack = []
    while True:
        t = type(v)
        if t is tuple and len(v) == 1:
            x = v[0]
            if x[-1] == 'y' or x == 'NaN':
                v = float(x) if pc is None else pc(x)
            elif '.' in x or 'e' in x or 'E' in x:
                v = float(x) if pf is None else pf(x)
            else:
                v = int(x) if pi is None else pi(x)
        elif (t is list or t is tuple) and v:
            stack.append((v, []))
            v = v[0]
            continue
        elif t is tuple:
            v = ph([]) if ph is not None else {} if oh is None else oh({})
        while stack:
            node, out = stack[-1]
            out.append(v)
            n = len(out)
            if n < len(node):
                v = node[n]
                break
            stack.pop()
            if type(node) is list:
                v = out
            else:
                pairs = list(zip(out[::2], out[1::2]))
                if ph is not None:
                    v = ph(pairs)
                else:
                    v = dict(pairs)
                    if oh is not None:
                        v = oh(v)
        else:
            return v


def load(fp, *, cls=None, object_hook=None, parse_float=None, parse_int=None, parse_constant=None,
         object_pairs_hook=None, **kw):
    return loads(fp.read(), cls=cls, object_hook=object_hook, parse_float=parse_float,
                 parse_int=parse_int, parse_constant=parse_constant,
                 object_pairs_hook=object_pairs_hook, **kw)


# ------------------------------------------------------------- asyncio
# `asyncio` (docs/spec.md §8.5): gather, sleep, create_task, the
# Task/Future methods and cancellation are native; these follow CPython
# 3.14's runners.py, locks.py, tasks.py and timeouts.py over the native
# scheduler.


__name__ = 'asyncio.futures'


def Future():
    import _mm
    return _mm.aio_future()


__name__ = 'asyncio.locks'


def _aio_value(name, args, kwargs):
    # `__init__(self, value=1)`'s argument binding and messages.
    if len(args) > 1:
        raise TypeError(f'{name}() takes from 1 to 2 positional arguments but {len(args) + 1} were given')
    for k in kwargs:
        if k != 'value':
            raise TypeError(f"{name}() got an unexpected keyword argument '{k}'")
        if args:
            raise TypeError(f"{name}() got multiple values for argument 'value'")
        return kwargs[k]
    return args[0] if args else 1


class Semaphore:

    def __init__(self, *args, **kwargs):
        value = _aio_value(f'{type(self).__name__}.__init__', args, kwargs)
        if value < 0:
            raise ValueError('Semaphore initial value must be >= 0')
        self._waiters = None
        self._value = value

    def __repr__(self):
        extra = 'locked' if self.locked() else f'unlocked, value:{self._value}'
        if self._waiters:
            extra = f'{extra}, waiters:{len(self._waiters)}'
        return f'<{type(self).__module__}.{type(self).__qualname__} object [{extra}]>'

    def locked(self):
        return self._value == 0 or any(not w.cancelled() for w in (self._waiters or ()))

    async def acquire(self):
        import _mm
        if not self.locked():
            self._value -= 1
            return True
        if self._waiters is None:
            self._waiters = []
        fut = _mm.aio_future()
        self._waiters.append(fut)
        try:
            try:
                await fut
            finally:
                self._waiters.remove(fut)
        except CancelledError:
            if fut.done() and not fut.cancelled():
                self._value += 1
            raise
        finally:
            while self._value > 0:
                if not self._wake_up_next():
                    break
        return True

    def release(self):
        self._value += 1
        self._wake_up_next()

    def _wake_up_next(self):
        import _mm
        if not self._waiters:
            return False
        for fut in self._waiters:
            if not fut.done():
                self._value -= 1
                _mm.aio_set(fut, True)
                return True
        return False

    async def __aenter__(self):
        await self.acquire()
        return None

    async def __aexit__(self, exc_type, exc, tb):
        self.release()


class BoundedSemaphore(Semaphore):

    def __init__(self, *args, **kwargs):
        value = _aio_value(f'{type(self).__name__}.__init__', args, kwargs)
        self._bound_value = value
        Semaphore.__init__(self, value)

    def release(self):
        if self._value >= self._bound_value:
            raise ValueError('BoundedSemaphore released too many times')
        Semaphore.release(self)


def _aio_opt(qual, args, kwargs, names, defaults):
    # Optional parameters of a prelude method (class units take none).
    if len(args) > len(names):
        raise TypeError(f'{qual}() takes from 1 to {len(names) + 1} positional arguments '
                        f'but {len(args) + 1} were given')
    out = list(args) + list(defaults[len(args):])
    for k in kwargs:
        if k not in names:
            raise TypeError(f"{qual}() got an unexpected keyword argument '{k}'")
        i = names.index(k)
        if i < len(args):
            raise TypeError(f"{qual}() got multiple values for argument '{k}'")
        out[i] = kwargs[k]
    return out


class Lock:

    def __init__(self):
        self._waiters = None
        self._locked = False

    def __repr__(self):
        extra = 'locked' if self._locked else 'unlocked'
        if self._waiters:
            extra = f'{extra}, waiters:{len(self._waiters)}'
        return f'<{type(self).__module__}.{type(self).__qualname__} object at {id(self):#x} [{extra}]>'

    def locked(self):
        return self._locked

    async def acquire(self):
        import _mm
        if not self._locked and (self._waiters is None or all(w.cancelled() for w in self._waiters)):
            self._locked = True
            return True
        if self._waiters is None:
            self._waiters = []
        fut = _mm.aio_future()
        self._waiters.append(fut)
        try:
            try:
                await fut
            finally:
                self._waiters.remove(fut)
        except CancelledError:
            if not self._locked:
                self._wake_up_first()
            raise
        self._locked = True
        return True

    def release(self):
        if self._locked:
            self._locked = False
            self._wake_up_first()
        else:
            raise RuntimeError('Lock is not acquired.')

    def _wake_up_first(self):
        if not self._waiters:
            return
        fut = self._waiters[0]
        if not fut.done():
            fut.set_result(True)

    async def __aenter__(self):
        await self.acquire()
        return None

    async def __aexit__(self, exc_type, exc, tb):
        self.release()


class Event:

    def __init__(self):
        self._waiters = []
        self._value = False

    def __repr__(self):
        extra = 'set' if self._value else 'unset'
        if self._waiters:
            extra = f'{extra}, waiters:{len(self._waiters)}'
        return f'<{type(self).__module__}.{type(self).__qualname__} object at {id(self):#x} [{extra}]>'

    def is_set(self):
        return self._value

    def set(self):
        if not self._value:
            self._value = True
            for fut in self._waiters:
                if not fut.done():
                    fut.set_result(True)

    def clear(self):
        self._value = False

    async def wait(self):
        import _mm
        if self._value:
            return True
        fut = _mm.aio_future()
        self._waiters.append(fut)
        try:
            await fut
            return True
        finally:
            self._waiters.remove(fut)


class Condition:

    def __init__(self, *args, **kwargs):
        lock, = _aio_opt('Condition.__init__', args, kwargs, ('lock',), (None,))
        if lock is None:
            lock = Lock()
        self._lock = lock
        self.locked = lock.locked
        self.acquire = lock.acquire
        self.release = lock.release
        self._waiters = []

    def __repr__(self):
        extra = 'locked' if self.locked() else 'unlocked'
        if self._waiters:
            extra = f'{extra}, waiters:{len(self._waiters)}'
        return f'<{type(self).__module__}.{type(self).__qualname__} object at {id(self):#x} [{extra}]>'

    async def __aenter__(self):
        await self.acquire()
        return None

    async def __aexit__(self, exc_type, exc, tb):
        self.release()

    async def wait(self):
        import _mm
        if not self.locked():
            raise RuntimeError('cannot wait on un-acquired lock')
        fut = _mm.aio_future()
        self.release()
        try:
            try:
                self._waiters.append(fut)
                try:
                    await fut
                    return True
                finally:
                    self._waiters.remove(fut)
            finally:
                err = None
                while True:
                    try:
                        await self.acquire()
                        break
                    except CancelledError as e:
                        err = e
                if err is not None:
                    try:
                        raise err
                    finally:
                        err = None
        except BaseException:
            self._notify(1)
            raise

    async def wait_for(self, predicate):
        result = predicate()
        while not result:
            await self.wait()
            result = predicate()
        return result

    def notify(self, *args, **kwargs):
        n, = _aio_opt('Condition.notify', args, kwargs, ('n',), (1,))
        if not self.locked():
            raise RuntimeError('cannot notify on un-acquired lock')
        self._notify(n)

    def _notify(self, n):
        idx = 0
        for fut in self._waiters:
            if idx >= n:
                break
            if not fut.done():
                idx += 1
                fut.set_result(False)

    def notify_all(self):
        self.notify(len(self._waiters))


class Barrier:
    # CPython's locks.Barrier; `_state` is the _BarrierState value.

    def __init__(self, parties):
        if parties < 1:
            raise ValueError('parties must be >= 1')
        self._cond = Condition()
        self._parties = parties
        self._state = 'filling'
        self._count = 0

    def __repr__(self):
        extra = self._state
        if not self.broken:
            extra += f', waiters:{self.n_waiting}/{self.parties}'
        return f'<{type(self).__module__}.{type(self).__qualname__} object at {id(self):#x} [{extra}]>'

    async def __aenter__(self):
        return await self.wait()

    async def __aexit__(self, *args):
        pass

    async def wait(self):
        async with self._cond:
            await self._block()
            try:
                index = self._count
                self._count += 1
                if index + 1 == self._parties:
                    await self._release()
                else:
                    await self._wait()
                return index
            finally:
                self._count -= 1
                self._exit()

    async def _block(self):
        await self._cond.wait_for(lambda: self._state not in ('draining', 'resetting'))
        if self._state == 'broken':
            raise BrokenBarrierError('Barrier aborted')

    async def _release(self):
        self._state = 'draining'
        self._cond.notify_all()

    async def _wait(self):
        await self._cond.wait_for(lambda: self._state != 'filling')
        if self._state in ('broken', 'resetting'):
            raise BrokenBarrierError('Abort or reset of barrier')

    def _exit(self):
        if self._count == 0:
            if self._state in ('resetting', 'draining'):
                self._state = 'filling'
            self._cond.notify_all()

    async def reset(self):
        async with self._cond:
            if self._count > 0:
                if self._state != 'resetting':
                    self._state = 'resetting'
            else:
                self._state = 'filling'
            self._cond.notify_all()

    async def abort(self):
        async with self._cond:
            self._state = 'broken'
            self._cond.notify_all()

    @property
    def parties(self):
        return self._parties

    @property
    def n_waiting(self):
        if self._state == 'filling':
            return self._count
        return 0

    @property
    def broken(self):
        return self._state == 'broken'


class BrokenBarrierError(RuntimeError):
    __module__ = 'asyncio.exceptions'


__name__ = 'asyncio.runners'


def run(main):
    # The main task runs in a copy of the caller's context.
    import _mm
    if not _mm.aio_running():
        _UnixSelectorEventLoop._mm_the = None
    return _aio_run_in(main, _mm.ctx_copy(), False)


def _aio_run_in(main, ctx, open):
    # `open`: run_until_complete(), which leaves the loop open (2: of a
    # future, which takes no task of its own).
    import _mm
    old = _mm.ctx_swap(ctx)
    try:
        return _mm.aio_run(main, _aio_main(main, open), open)
    finally:
        _mm.ctx_swap(old)


async def _aio_await(aw):
    return await aw


async def _aio_noop():
    pass


class Runner:
    # CPython's runners.Runner over the one loop. Each run() is a whole
    # asyncio.run(): the tasks it leaves are cancelled when it ends, where
    # CPython keeps them for the next run() or close().

    def __init__(self, **kwargs):
        debug, loop_factory = _aio_opt('Runner.__init__', (), kwargs,
                                       ('debug', 'loop_factory'), (None, None))
        self._state = 'created'
        self._loop_factory = loop_factory
        self._loop = None
        self._context = None
        self._set_event_loop = False

    def __enter__(self):
        self._lazy_init()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()

    def close(self):
        if self._state != 'initialized':
            return
        # shutdown_asyncgens() and shutdown_default_executor(), each a task.
        import _mm
        self._loop._mm_run(_aio_noop(), _mm.ctx_copy(), True)
        self._loop._mm_run(_aio_noop(), _mm.ctx_copy(), True)
        if self._set_event_loop:
            set_event_loop(None)
        self._loop.close()
        self._loop = None
        self._state = 'closed'

    def get_loop(self):
        self._lazy_init()
        return self._loop

    def run(self, coro, **kwargs):
        import _mm
        context, = _aio_opt('Runner.run', (), kwargs, ('context',), (None,))
        if _mm.aio_running():
            raise RuntimeError('Runner.run() cannot be called from a running event loop')
        self._lazy_init()
        if type(coro) is not coroutine:
            if isfuture(coro) or hasattr(coro, '__await__'):
                coro = _aio_await(coro)
            else:
                raise TypeError('An asyncio.Future, a coroutine or an awaitable is required')
        if context is None:
            context = self._context
        return self._loop._mm_run(coro, context._data, True)

    def _lazy_init(self):
        if self._state == 'closed':
            raise RuntimeError('Runner is closed')
        if self._state == 'initialized':
            return
        if self._loop_factory is None:
            self._loop = new_event_loop()
            if not self._set_event_loop:
                set_event_loop(self._loop)
                self._set_event_loop = True
        else:
            self._loop = self._loop_factory()
        import contextvars
        self._context = contextvars.copy_context()
        self._state = 'initialized'


async def _aio_main(main, open):
    # `asyncio.run`'s task, then `Runner.close()`: the loop stops at the end
    # of the iteration after `main` ends, then the tasks left are cancelled
    # and run until their cancellation is done.
    import _mm
    try:
        try:
            r = await main
        except BaseException as e:
            _mm.aio_main_end(False, e)
            raise
        return _mm.aio_main_end(True, r)
    finally:
        _aio_unretrieved()
        await _mm.aio_stop(2)
        tasks = _mm.aio_tasks()
        if tasks:
            for t in tasks:
                t.cancel()
            import asyncio
            await asyncio.gather(*tasks, return_exceptions=True)
            await _mm.aio_stop(1)
            for t in tasks:
                if not t.cancelled() and t.exception() is not None:
                    _aio_report(t)
            # `shutdown_asyncgens()` and `shutdown_default_executor()` run
            # the loop two iterations each.
            if not open:
                await _mm.aio_stop(4)
        _aio_unretrieved()
        # A KeyboardInterrupt or SystemExit a task raised leaves the run.
        _mm.aio_escape()


def _aio_unretrieved():
    # What CPython's Future.__del__ reports when the last reference goes.
    import _mm
    for f in _mm.aio_unretrieved():
        kind = 'Task' if type(f) is Task else 'Future'
        _aio_exc_handler({'message': f'{kind} exception was never retrieved',
                          'exception': f.exception(), 'future': f})


def _aio_report(t):
    import sys
    import traceback
    exc = t.exception()
    sys.stderr.write('unhandled exception during asyncio.run() shutdown\ntask: ' + repr(t) + '\n'
                     + ''.join(traceback.format_exception(exc)))


__name__ = 'asyncio.tasks'


def _aio_ensure(aw):
    import _mm
    if type(aw) is Task or type(aw) is _AioFuture:
        return aw
    return _mm.aio_spawn(aw)


async def _aio_wait(delay):
    import _mm
    await _mm.aio_deadline(delay)


async def _aio_timer(delay, task, fired):
    import _mm
    await _mm.aio_deadline(delay)
    fired.append(True)
    task.cancel()


def _aio_insert_timeout(exc_val):
    while exc_val.__context__ is not None:
        if isinstance(exc_val.__context__, CancelledError):
            te = TimeoutError()
            te.__context__ = te.__cause__ = exc_val.__context__
            exc_val.__context__ = te
            break
        exc_val = exc_val.__context__


async def wait_for(fut, timeout):
    import _mm
    # CPython 3.12+: the awaitable runs in the calling task, and the
    # deadline cancels that task (timeouts.py); the CancelledError becomes
    # TimeoutError unless another cancel is pending.
    if timeout is not None and timeout <= 0:
        fut = _aio_ensure(fut)
        if fut.done():
            return fut.result()
        fut.cancel()
        await _mm.aio_first([fut])
        try:
            return fut.result()
        except CancelledError as exc:
            raise TimeoutError from exc
    if timeout is None:
        return await fut
    task = _mm.aio_current_task()
    cancelling = task.cancelling()
    fired = []
    timer = _mm.aio_spawn_internal(_aio_timer(timeout, task, fired))
    try:
        r = await fut
    except BaseException as exc:
        timer.cancel()
        if fired and task.uncancel() <= cancelling:
            if isinstance(exc, CancelledError):
                raise TimeoutError from exc
            _aio_insert_timeout(exc)
        raise
    timer.cancel()
    if fired:
        task.uncancel()
    return r


class _AsCompletedIterator:
    # CPython's: results come in the order the awaitables finish.

    def __init__(self, aws, timeout):
        import _mm
        todo = []
        for aw in aws:
            if not any(aw is t for t in todo):
                todo.append(aw)
        self._todo = [_aio_ensure(aw) for aw in todo]
        self._todo_left = len(self._todo)
        self._timer = None
        if self._todo and timeout is not None:
            self._timer = _mm.aio_spawn_internal(_aio_wait(timeout))

    def __aiter__(self):
        return self

    def __iter__(self):
        return self

    async def __anext__(self):
        if not self._todo_left:
            raise StopAsyncIteration
        self._todo_left -= 1
        return await self._wait_for_one()

    def __next__(self):
        if not self._todo_left:
            raise StopIteration
        self._todo_left -= 1
        return self._wait_for_result()

    async def _wait_for_result(self):
        return (await self._wait_for_one()).result()

    async def _wait_for_one(self):
        import _mm
        if self._timer is None or not self._timer.done():
            waits = self._todo if self._timer is None else self._todo + [self._timer]
            f = await _mm.aio_first(waits)
        else:
            f = self._timer
        if f is self._timer:
            self._todo.clear()
            raise TimeoutError
        self._todo.remove(f)
        if not self._todo and self._timer is not None:
            self._timer.cancel()
        return f


def as_completed(fs, *, timeout=None):
    if type(fs) in (Task, _AioFuture, coroutine):
        raise TypeError(f'expects an iterable of awaitables, not {type(fs).__name__}')
    return _AsCompletedIterator(fs, timeout)


__name__ = 'asyncio.queues'


class QueueEmpty(Exception):
    __module__ = 'asyncio.queues'


class QueueFull(Exception):
    __module__ = 'asyncio.queues'


class QueueShutDown(Exception):
    __module__ = 'asyncio.queues'


class Queue:

    def __class_getitem__(cls, item):
        return GenericAlias(cls, item)

    def __init__(self, *args, **kwargs):
        maxsize, = _aio_opt(f'{type(self).__name__}.__init__', args, kwargs, ('maxsize',), (0,))
        self._maxsize = maxsize
        self._getters = []
        self._putters = []
        self._unfinished_tasks = 0
        self._finished = Event()
        self._finished.set()
        self._init(maxsize)
        self._is_shutdown = False

    def _init(self, maxsize):
        self._queue = []

    def _get(self):
        return self._queue.pop(0)

    def _put(self, item):
        self._queue.append(item)

    def _wakeup_next(self, waiters):
        while waiters:
            waiter = waiters.pop(0)
            if not waiter.done():
                waiter.set_result(None)
                break

    def __repr__(self):
        return f'<{type(self).__name__} at {id(self):#x} {self._format()}>'

    def __str__(self):
        return f'<{type(self).__name__} {self._format()}>'

    def _format(self):
        result = f'maxsize={self._maxsize!r}'
        if getattr(self, '_queue', None):
            result += f' _queue={list(self._queue)!r}'
        if self._getters:
            result += f' _getters[{len(self._getters)}]'
        if self._putters:
            result += f' _putters[{len(self._putters)}]'
        if self._unfinished_tasks:
            result += f' tasks={self._unfinished_tasks}'
        if self._is_shutdown:
            result += ' shutdown'
        return result

    def qsize(self):
        return len(self._queue)

    @property
    def maxsize(self):
        return self._maxsize

    def empty(self):
        return not self._queue

    def full(self):
        if self._maxsize <= 0:
            return False
        return self.qsize() >= self._maxsize

    async def put(self, item):
        import _mm
        while self.full():
            if self._is_shutdown:
                raise QueueShutDown
            putter = _mm.aio_future()
            self._putters.append(putter)
            try:
                await putter
            except:
                putter.cancel()
                try:
                    self._putters.remove(putter)
                except ValueError:
                    pass
                if not self.full() and not putter.cancelled():
                    self._wakeup_next(self._putters)
                raise
        return self.put_nowait(item)

    def put_nowait(self, item):
        if self._is_shutdown:
            raise QueueShutDown
        if self.full():
            raise QueueFull
        self._put(item)
        self._unfinished_tasks += 1
        self._finished.clear()
        self._wakeup_next(self._getters)

    async def get(self):
        import _mm
        while self.empty():
            if self._is_shutdown and self.empty():
                raise QueueShutDown
            getter = _mm.aio_future()
            self._getters.append(getter)
            try:
                await getter
            except:
                getter.cancel()
                try:
                    self._getters.remove(getter)
                except ValueError:
                    pass
                if not self.empty() and not getter.cancelled():
                    self._wakeup_next(self._getters)
                raise
        return self.get_nowait()

    def get_nowait(self):
        if self.empty():
            if self._is_shutdown:
                raise QueueShutDown
            raise QueueEmpty
        item = self._get()
        self._wakeup_next(self._putters)
        return item

    def task_done(self):
        if self._unfinished_tasks <= 0:
            raise ValueError('task_done() called too many times')
        self._unfinished_tasks -= 1
        if self._unfinished_tasks == 0:
            self._finished.set()

    async def join(self):
        if self._unfinished_tasks > 0:
            await self._finished.wait()

    def shutdown(self, *args, **kwargs):
        immediate, = _aio_opt('Queue.shutdown', args, kwargs, ('immediate',), (False,))
        self._is_shutdown = True
        if immediate:
            while not self.empty():
                self._get()
                if self._unfinished_tasks > 0:
                    self._unfinished_tasks -= 1
            if self._unfinished_tasks == 0:
                self._finished.set()
        while self._getters:
            getter = self._getters.pop(0)
            if not getter.done():
                getter.set_result(None)
        while self._putters:
            putter = self._putters.pop(0)
            if not putter.done():
                putter.set_result(None)


class PriorityQueue(Queue):

    def _init(self, maxsize):
        self._queue = []

    def _put(self, item):
        import heapq
        heapq.heappush(self._queue, item)

    def _get(self):
        import heapq
        return heapq.heappop(self._queue)


class LifoQueue(Queue):

    def _init(self, maxsize):
        self._queue = []

    def _put(self, item):
        self._queue.append(item)

    def _get(self):
        return self._queue.pop()


__name__ = 'asyncio.events'


class Handle:

    def __init__(self, callback, args, context):
        import _mm
        self._callback = callback
        self._args = args
        self._cancelled = False
        # The context it runs in: a copy of the scheduler's, as CPython's.
        self._ctx = _mm.ctx_copy() if context is None else context._data

    def __repr__(self):
        if self._cancelled:
            return f'<{type(self).__name__} cancelled>'
        return f'<{type(self).__name__} {_aio_cb_name(self._callback)}>'

    def cancel(self):
        if not self._cancelled:
            self._cancelled = True
            self._callback = None
            self._args = None

    def cancelled(self):
        return self._cancelled

    def _run(self):
        import _mm
        if not self._cancelled:
            old = _mm.ctx_swap(self._ctx)
            try:
                _aio_callback(self._callback, *self._args, _aio_h=self)
            finally:
                _mm.ctx_swap(old)


class TimerHandle(Handle):

    def __init__(self, when, callback, args, context):
        Handle.__init__(self, callback, args, context)
        self._when = when
        self._task = None

    def __repr__(self):
        if self._cancelled:
            return f'<TimerHandle cancelled when={self._when}>'
        return f'<TimerHandle when={self._when} {_aio_cb_name(self._callback)}>'

    def when(self):
        return self._when

    def cancel(self):
        Handle.cancel(self)
        if self._task is not None:
            self._task.cancel()
            self._task = None


def _aio_cb_name(f):
    # CPython 3.14 shows no arguments outside debug mode.
    name = getattr(f, '__qualname__', None) or getattr(f, '__name__', None) or repr(f)
    return name + '()'


def _aio_callback(f, *args, _aio_h=None):
    # A loop callback: what it raises goes to the exception handler.
    try:
        f(*args)
    except (SystemExit, KeyboardInterrupt):
        raise
    except BaseException as e:
        _aio_exc_handler({'message': f'Exception in callback {_aio_cb_name(f)}', 'exception': e,
                          'handle': _aio_h or Handle(f, (), None)})


def _aio_exc_handler(context):
    import sys
    import traceback
    lines = [context.get('message') or 'Unhandled exception in event loop']
    for key in sorted(context):
        if key not in ('message', 'exception'):
            lines.append(f'{key}: {context[key]!r}')
    exc = context.get('exception')
    text = '\n'.join(lines) + '\n'
    if exc is not None:
        text += ''.join(traceback.format_exception(exc))
    sys.stderr.write(text)


async def _aio_later(h):
    import _mm
    await _mm.aio_deadline(h._when - _mm.aio_time())
    h._task = None
    h._run()


def _aio_call_at(when, callback, args, kwargs):
    import _mm
    for k in kwargs:
        if k != 'context':
            raise TypeError(f"call_at() got an unexpected keyword argument '{k}'")
    h = TimerHandle(when, callback, args, kwargs.get('context'))
    h._task = _mm.aio_spawn_internal(_aio_later(h))
    return h


class _UnixSelectorEventLoop:
    # The one event loop: the engine's scheduler on the virtual clock.
    __module__ = 'asyncio.unix_events'

    def __repr__(self):
        return f'<_UnixSelectorEventLoop running={self.is_running()} closed=False debug=False>'

    def time(self):
        import _mm
        return _mm.aio_time()

    def is_running(self):
        import _mm
        return _mm.aio_running() and _aio_the_loop() is self

    def is_closed(self):
        return getattr(self, '_mm_closed', False)

    def get_debug(self):
        return False

    def create_future(self):
        import _mm
        return _mm.aio_future()

    def create_task(self, coro, **kwargs):
        import asyncio
        return asyncio.create_task(coro, **kwargs)

    def call_soon(self, callback, *args, **kwargs):
        import _mm
        for k in kwargs:
            if k != 'context':
                raise TypeError(f"call_soon() got an unexpected keyword argument '{k}'")
        h = Handle(callback, args, kwargs.get('context'))
        _mm.aio_soon(h._run)
        return h

    def call_later(self, delay, callback, *args, **kwargs):
        return _aio_call_at(self.time() + delay, callback, args, kwargs)

    def call_at(self, when, callback, *args, **kwargs):
        return _aio_call_at(when, callback, args, kwargs)

    def call_exception_handler(self, context):
        _aio_exc_handler(context)

    def run_until_complete(self, future):
        # A whole asyncio.run(): the tasks it leaves are cancelled at its
        # end, where CPython keeps them pending in the loop.
        import _mm
        open = True
        if type(future) is not coroutine:
            open = 2 if isfuture(future) else True
            if not (open == 2 or hasattr(future, '__await__')):
                raise TypeError('An asyncio.Future, a coroutine or an awaitable is required')
            future = _aio_await(future)
        return self._mm_run(future, _mm.ctx_copy(), open)

    def _mm_run(self, main, ctx, open):
        import _mm
        if self.is_closed():
            raise RuntimeError('Event loop is closed')
        if _mm.aio_running():
            if self.is_running():
                raise RuntimeError('This event loop is already running')
            raise RuntimeError('Cannot run the event loop while another loop is running')
        _UnixSelectorEventLoop._mm_the = self
        return _aio_run_in(main, ctx, open)

    def run_forever(self):
        if self.is_running():
            raise RuntimeError('This event loop is already running')
        raise RuntimeError('micro-monty: loop.run_forever() is not available; '
                           'use loop.run_until_complete() or asyncio.run()')

    def stop(self):
        raise RuntimeError('micro-monty: the event loop stops when asyncio.run() returns')

    def close(self):
        if self.is_running():
            raise RuntimeError('Cannot close a running event loop')
        self._mm_closed = True

    def run_in_executor(self, executor, func, *args):
        _aio_unavailable('loop.run_in_executor()', 'processes')

    def subprocess_exec(self, *args, **kwargs):
        _aio_unavailable('loop.subprocess_exec()', 'processes')

    def subprocess_shell(self, *args, **kwargs):
        _aio_unavailable('loop.subprocess_shell()', 'processes')

    def create_connection(self, *args, **kwargs):
        _aio_unavailable('loop.create_connection()', 'network')

    def create_server(self, *args, **kwargs):
        _aio_unavailable('loop.create_server()', 'network')

    def getaddrinfo(self, *args, **kwargs):
        _aio_unavailable('loop.getaddrinfo()', 'network')


EventLoop = _UnixSelectorEventLoop


def _aio_unavailable(what, group):
    import _mm
    hint = _mm.hint(group)
    raise RuntimeError(f'micro-monty: {what} is not available' + ('. ' + hint if hint else ''))


def _aio_the_loop():
    import _mm
    if not _mm.aio_running():
        raise RuntimeError('no running event loop')
    L = _UnixSelectorEventLoop
    loop = getattr(L, '_mm_the', None)
    if loop is None:
        loop = L()
        L._mm_the = loop
    return loop


def get_running_loop():
    return _aio_the_loop()


def get_event_loop():
    import _mm
    if _mm.aio_running():
        return _aio_the_loop()
    loop = getattr(_UnixSelectorEventLoop, '_mm_set', None)
    if loop is None:
        raise RuntimeError("There is no current event loop in thread 'MainThread'.")
    return loop


def new_event_loop():
    # A loop object over the one engine loop; run_until_complete() runs it.
    return _UnixSelectorEventLoop()


def set_event_loop(loop):
    if loop is not None and not isinstance(loop, _UnixSelectorEventLoop):
        raise TypeError(f"loop must be an instance of AbstractEventLoop or None, not '{type(loop).__name__}'")
    _UnixSelectorEventLoop._mm_set = loop


def _set_event_loop_policy(policy):
    if policy is not None:
        raise RuntimeError('micro-monty: event loop policies are not available')
    _UnixSelectorEventLoop._mm_set = None


__name__ = 'asyncio.futures'


def _aio_set_exception(fut, exc):
    import _mm
    if isinstance(exc, type):
        exc = exc()
    if type(exc) is StopIteration:
        new = RuntimeError('StopIteration interacts badly with generators and cannot be raised into a Future')
        new.__cause__ = exc
        new.__context__ = exc
        exc = new
    _mm.aio_set_exc(fut, exc)


def isfuture(obj):
    return type(obj) is Task or type(obj) is _AioFuture or type(obj).__name__ == '_GatheringFuture'


__name__ = 'asyncio.tasks'


FIRST_COMPLETED = 'FIRST_COMPLETED'
FIRST_EXCEPTION = 'FIRST_EXCEPTION'
ALL_COMPLETED = 'ALL_COMPLETED'


def _aio_create_ctx(coro, name, context):
    import _mm
    import asyncio
    t = asyncio.create_task(coro, name=name)
    _mm.aio_task_ctx(t, context._data)
    return t


def _aio_create_named(coro, name):
    import asyncio
    return asyncio.create_task(coro, name=str(name))


def _aio_set_name(task, value):
    import _mm
    _mm.aio_name_set(task, str(value))


def current_task(loop=None):
    import _mm
    _aio_the_loop()
    return _mm.aio_current_task()


def all_tasks(loop=None):
    import _mm
    _aio_the_loop()
    return set(_mm.aio_all_tasks())


def ensure_future(coro_or_future, *, loop=None):
    import asyncio
    if isfuture(coro_or_future):
        return coro_or_future
    if type(coro_or_future) is coroutine:
        return asyncio.create_task(coro_or_future)
    if hasattr(coro_or_future, '__await__'):
        async def _wrap_awaitable(awaitable):
            return await awaitable
        return asyncio.create_task(_wrap_awaitable(coro_or_future))
    raise TypeError('An asyncio.Future, a coroutine or an awaitable is required')


def _release_waiter(waiter, *args):
    if not waiter.done():
        waiter.set_result(None)


async def wait(fs, *, timeout=None, return_when='ALL_COMPLETED'):
    if isfuture(fs) or type(fs) is coroutine:
        raise TypeError(f'expect a list of futures, not {type(fs).__name__}')
    if not fs:
        raise ValueError('Set of Tasks/Futures is empty.')
    if return_when not in ('FIRST_COMPLETED', 'FIRST_EXCEPTION', 'ALL_COMPLETED'):
        raise ValueError(f'Invalid return_when value: {return_when}')
    fs = set(fs)
    if any(type(f) is coroutine for f in fs):
        raise TypeError('Passing coroutines is forbidden, use tasks explicitly.')
    loop = _aio_the_loop()
    waiter = loop.create_future()
    timeout_handle = None
    if timeout is not None:
        timeout_handle = loop.call_later(timeout, _release_waiter, waiter)
    counter = len(fs)

    def _on_completion(f):
        nonlocal counter
        counter -= 1
        if (counter <= 0 or return_when == 'FIRST_COMPLETED'
                or (return_when == 'FIRST_EXCEPTION' and not f.cancelled() and f.exception() is not None)):
            if timeout_handle is not None:
                timeout_handle.cancel()
            if not waiter.done():
                waiter.set_result(None)
    for f in fs:
        f.add_done_callback(_on_completion)
    try:
        await waiter
    finally:
        if timeout_handle is not None:
            timeout_handle.cancel()
        for f in fs:
            f.remove_done_callback(_on_completion)
    done, pending = set(), set()
    for f in fs:
        if f.done():
            done.add(f)
        else:
            pending.add(f)
    return done, pending


def _aio_log_on_exception(fut):
    if fut.cancelled():
        return
    exc = fut.exception()
    if exc is None:
        return
    _aio_exc_handler({'message': f'{type(exc).__name__} exception in shielded future',
                      'exception': exc, 'future': fut})


def shield(arg):
    import _mm
    inner = ensure_future(arg)
    if inner.done():
        return inner
    outer = _mm.aio_future()

    def _inner_done_callback(inner):
        if outer.cancelled():
            return
        if inner.cancelled():
            outer.cancel()
        else:
            exc = inner.exception()
            if exc is not None:
                outer.set_exception(exc)
            else:
                outer.set_result(inner.result())

    def _outer_done_callback(outer):
        if not inner.done():
            inner.remove_done_callback(_inner_done_callback)
            inner.remove_done_callback(_aio_log_on_exception)
            inner.add_done_callback(_aio_log_on_exception)
    inner.add_done_callback(_inner_done_callback)
    outer.add_done_callback(_outer_done_callback)
    return outer


__name__ = 'asyncio.taskgroups'


class TaskGroup:

    def __init__(self):
        self._entered = False
        self._exiting = False
        self._aborting = False
        self._loop = None
        self._parent_task = None
        self._parent_cancel_requested = False
        self._tasks = {}
        self._errors = []
        self._base_error = None
        self._on_completed_fut = None

    def __repr__(self):
        info = ['']
        if self._tasks:
            info.append(f'tasks={len(self._tasks)}')
        if self._errors:
            info.append(f'errors={len(self._errors)}')
        if self._aborting:
            info.append('cancelling')
        elif self._entered:
            info.append('entered')
        info_str = ' '.join(info)
        return f'<TaskGroup{info_str}>'

    async def __aenter__(self):
        if self._entered:
            raise RuntimeError(f'TaskGroup {self!r} has already been entered')
        if self._loop is None:
            self._loop = _aio_the_loop()
        self._parent_task = current_task()
        if self._parent_task is None:
            raise RuntimeError(f'TaskGroup {self!r} cannot determine the parent task')
        self._entered = True
        return self

    async def __aexit__(self, et, exc, tb):
        tb = None
        try:
            return await self._aexit(et, exc)
        finally:
            self._parent_task = None
            self._errors = None
            self._base_error = None
            exc = None

    async def _aexit(self, et, exc):
        self._exiting = True
        if exc is not None and self._is_base_error(exc) and self._base_error is None:
            self._base_error = exc
        if et is not None and issubclass(et, CancelledError):
            propagate_cancellation_error = exc
        else:
            propagate_cancellation_error = None
        if et is not None:
            if not self._aborting:
                self._abort()
        while self._tasks:
            if self._on_completed_fut is None:
                self._on_completed_fut = self._loop.create_future()
            try:
                await self._on_completed_fut
            except CancelledError as ex:
                if not self._aborting:
                    propagate_cancellation_error = ex
                    self._abort()
            self._on_completed_fut = None
        if self._base_error is not None:
            try:
                raise self._base_error
            finally:
                exc = None
        if self._parent_cancel_requested:
            if self._parent_task.uncancel() == 0:
                propagate_cancellation_error = None
        try:
            if propagate_cancellation_error is not None and not self._errors:
                try:
                    raise propagate_cancellation_error
                finally:
                    exc = None
        finally:
            propagate_cancellation_error = None
        if et is not None and not issubclass(et, CancelledError):
            self._errors.append(exc)
        if self._errors:
            if self._parent_task.cancelling():
                self._parent_task.uncancel()
                self._parent_task.cancel()
            try:
                raise BaseExceptionGroup('unhandled errors in a TaskGroup', self._errors) from None
            finally:
                exc = None

    def create_task(self, coro, **kwargs):
        if not self._entered:
            coro.close()
            raise RuntimeError(f'TaskGroup {self!r} has not been entered')
        if self._exiting and not self._tasks:
            coro.close()
            raise RuntimeError(f'TaskGroup {self!r} is finished')
        if self._aborting:
            coro.close()
            raise RuntimeError(f'TaskGroup {self!r} is shutting down')
        task = self._loop.create_task(coro, **kwargs)
        self._tasks[task] = None
        task.add_done_callback(self._on_task_done)
        return task

    def _is_base_error(self, exc):
        return isinstance(exc, (SystemExit, KeyboardInterrupt))

    def _abort(self):
        self._aborting = True
        for t in self._tasks:
            if not t.done():
                t.cancel()

    def _on_task_done(self, task):
        self._tasks.pop(task, None)
        if self._on_completed_fut is not None and not self._tasks:
            if not self._on_completed_fut.done():
                self._on_completed_fut.set_result(True)
        if task.cancelled():
            return
        exc = task.exception()
        if exc is None:
            return
        self._errors.append(exc)
        if self._is_base_error(exc) and self._base_error is None:
            self._base_error = exc
        if self._parent_task.done():
            self._loop.call_exception_handler({
                'message': f'Task {task!r} has errored out but its parent task {self._parent_task} is already completed',
                'exception': exc, 'task': task})
            return
        if not self._aborting and not self._parent_cancel_requested:
            self._abort()
            self._parent_cancel_requested = True
            self._parent_task.cancel()


__name__ = 'asyncio.timeouts'


class Timeout:

    def __init__(self, when):
        self._state = 'created'
        self._timeout_handler = None
        self._task = None
        self._when = when

    def when(self):
        return self._when

    def reschedule(self, when):
        if self._state != 'active':
            if self._state == 'created':
                raise RuntimeError('Timeout has not been entered')
            raise RuntimeError(f'Cannot change state of {self._state} Timeout')
        self._when = when
        if self._timeout_handler is not None:
            self._timeout_handler.cancel()
        if when is None:
            self._timeout_handler = None
        else:
            loop = _aio_the_loop()
            if when <= loop.time():
                self._timeout_handler = loop.call_soon(self._on_timeout)
            else:
                self._timeout_handler = loop.call_at(when, self._on_timeout)

    def expired(self):
        return self._state in ('expiring', 'expired')

    def __repr__(self):
        info = ['']
        if self._state == 'active':
            when = round(self._when, 3) if self._when is not None else None
            info.append(f'when={when}')
        info_str = ' '.join(info)
        return f'<Timeout [{self._state}]{info_str}>'

    async def __aenter__(self):
        if self._state != 'created':
            raise RuntimeError('Timeout has already been entered')
        task = current_task()
        if task is None:
            raise RuntimeError('Timeout should be used inside a task')
        self._state = 'active'
        self._task = task
        self._cancelling = self._task.cancelling()
        self.reschedule(self._when)
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self._timeout_handler is not None:
            self._timeout_handler.cancel()
            self._timeout_handler = None
        if self._state == 'expiring':
            self._state = 'expired'
            if self._task.uncancel() <= self._cancelling and exc_type is not None:
                if issubclass(exc_type, CancelledError):
                    raise TimeoutError from exc_val
                elif exc_val is not None:
                    _aio_insert_timeout(exc_val)
                    if isinstance(exc_val, ExceptionGroup):
                        for exc in exc_val.exceptions:
                            _aio_insert_timeout(exc)
        elif self._state == 'active':
            self._state = 'finished'
        return None

    def _on_timeout(self):
        self._task.cancel()
        self._state = 'expiring'
        self._timeout_handler = None


def timeout(delay):
    loop = _aio_the_loop()
    return Timeout(loop.time() + delay if delay is not None else None)


def timeout_at(when):
    return Timeout(when)


# ----------------------------------------------------------------- sys
# `sys` struct sequences (`version_info`, `float_info`, `flags`): tuples
# with field names, without tuple subclassing.

__name__ = 'sys'


def _ss_items(o):
    return o._t if isinstance(o, _StructSeq) else o


class _StructSeq:
    def __init__(self, name, fields, t):
        self._n = name
        self._f = fields
        self._t = t

    def __getattr__(self, a):
        f = self._f
        if a in f:
            return self._t[f.index(a)]
        raise AttributeError(f"'{self._n}' object has no attribute '{a}'")

    def __repr__(self):
        parts = [f'{k}={v!r}' for k, v in zip(self._f, self._t)]
        return f"{self._n}({', '.join(parts)})"

    def __len__(self):
        return len(self._t)

    def __getitem__(self, i):
        return self._t[i]

    def __iter__(self):
        return iter(self._t)

    def __contains__(self, x):
        return x in self._t

    def __hash__(self):
        return hash(self._t)

    def __eq__(self, o):
        return self._t == _ss_items(o)

    def __ne__(self, o):
        return self._t != _ss_items(o)

    def __lt__(self, o):
        return self._t < _ss_items(o)

    def __le__(self, o):
        return self._t <= _ss_items(o)

    def __gt__(self, o):
        return self._t > _ss_items(o)

    def __ge__(self, o):
        return self._t >= _ss_items(o)

    def __add__(self, o):
        return self._t + _ss_items(o)

    def __radd__(self, o):
        return _ss_items(o) + self._t

    def __mul__(self, n):
        return self._t * n

    def __rmul__(self, n):
        return self._t * n

    def __getnewargs__(self):
        return (self._t,)

    def count(self, x):
        return self._t.count(x)

    def index(self, x):
        return self._t.index(x)


_StructSeq.__name__ = 'structseq'
_version_info = _StructSeq('sys.version_info', ('major', 'minor', 'micro', 'releaselevel', 'serial'),
                           (3, 14, 7, 'final', 0))
_float_info = _StructSeq(
    'sys.float_info',
    ('max', 'max_exp', 'max_10_exp', 'min', 'min_exp', 'min_10_exp', 'dig', 'mant_dig', 'epsilon',
     'radix', 'rounds'),
    (1.7976931348623157e308, 1024, 308, 2.2250738585072014e-308, -1021, -307, 15, 53,
     2.220446049250313e-16, 2, 1))
_hash_info = _StructSeq(
    'sys.hash_info',
    ('width', 'modulus', 'inf', 'nan', 'imag', 'algorithm', 'hash_bits', 'seed_bits', 'cutoff'),
    (64, 2305843009213693951, 314159, 0, 1000003, 'siphash13', 64, 128, 0))
_flags = _StructSeq(
    'sys.flags',
    ('debug', 'inspect', 'interactive', 'optimize', 'dont_write_bytecode', 'no_user_site', 'no_site',
     'ignore_environment', 'verbose', 'bytes_warning', 'quiet', 'hash_randomization', 'isolated',
     'dev_mode', 'utf8_mode', 'warn_default_encoding', 'safe_path', 'int_max_str_digits'),
    (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, False, 0, 0, False, 4300))
_int_info = _StructSeq(
    'sys.int_info',
    ('bits_per_digit', 'sizeof_digit', 'default_max_str_digits', 'str_digits_check_threshold'),
    (30, 4, 4300, 640))
# Read by the engine's int/str conversions (0: no limit).
_int_max_str_digits = 4300


def get_int_max_str_digits():
    return _int_max_str_digits


def set_int_max_str_digits(maxdigits):
    global _int_max_str_digits
    n = maxdigits
    if type(n) is not int and type(n) is not bool:
        if not _has_index(n):
            raise TypeError(f"'{_type_name(n)}' object cannot be interpreted as an integer")
        n = _index(n)
    if n != 0 and n < 640:
        raise ValueError('maxdigits must be >= 640 or 0 for unlimited')
    _int_max_str_digits = n
    import sys
    sys.flags = _StructSeq('sys.flags', _flags._f, _flags._t[:-1] + (n,))


# ------------------------------------------------------- slot wrappers
# The twin of every builtin type's slot wrapper (`list.__repr__`, ...), for
# receivers holding user objects: `a` is (receiver, *args).


def _slot(name, a):
    if name == '__getitem__':
        return a[0][a[1]]
    if name == '__setitem__':
        a[0][a[1]] = a[2]
        return None
    if name == '__delitem__':
        del a[0][a[1]]
        return None
    if name == '__len__':
        return len(a[0])
    if name == '__contains__':
        return a[1] in a[0]
    if name == '__iter__':
        return iter(a[0])
    if name == '__repr__':
        return repr(a[0])
    if name == '__hash__':
        return hash(a[0])
    if name == '__eq__':
        return a[0] == a[1]
    if name == '__ne__':
        return a[0] != a[1]
    if name == '__lt__':
        return a[0] < a[1]
    if name == '__le__':
        return a[0] <= a[1]
    if name == '__gt__':
        return a[0] > a[1]
    return a[0] >= a[1]


# --------------------------------------------------------- host records
# A `HostValue::Record` from the host arrives as an instance of a named
# tuple class the engine builds from these methods (one class per name and
# field list, cached in `_host_records`); a named tuple sent to the host
# leaves as a `Record` (docs/spec.md §10.1).

_host_records = {}


def _rec_getattr(self, name):
    fields = type(self)._fields
    if name in fields:
        return self[fields.index(name)]
    raise AttributeError(f"'{type(self).__name__}' object has no attribute '{name}'")


def _rec_repr(self):
    parts = [f'{k}={v!r}' for k, v in zip(self._fields, self)]
    return f"{type(self).__name__}({', '.join(parts)})"


def _rec_asdict(self):
    return dict(zip(self._fields, self))


def _rec_replace(self, /, **kwds):
    values = [kwds.pop(k, v) for k, v in zip(self._fields, self)]
    if kwds:
        raise TypeError(f'Got unexpected field names: {list(kwds)!r}')
    return tuple.__new__(type(self), values)


def _rec_make(cls, iterable):
    result = tuple.__new__(cls, iterable)
    if len(result) != len(cls._fields):
        raise TypeError(f'Expected {len(cls._fields)} arguments, got {len(result)}')
    return result


def _rec_getnewargs(self):
    return (tuple(self),)


# ---------------------------------------------------------- metaclasses


def _build_meta(meta, name, bases, ns, kw, cell):
    # `class C(..., metaclass=M)`: the class is `M(name, bases, ns, **kw)`;
    # `type.__new__` fills the `__class__` cell it finds as `__classcell__`.
    if cell is not None:
        ns['__classcell__'] = cell
    cls = meta(name, bases, ns, **kw)
    if cell is not None:
        _cell_set(cell, cls)
    return cls


def _type_call(cls, args, kw):
    # `type.__call__`: `__new__`, then `__init__` on an instance of `cls`.
    obj = cls.__new__(cls, *args, **kw)
    if isinstance(obj, cls):
        init = type(obj).__init__
        if init is not object.__init__:
            init(obj, *args, **kw)
    return obj


# `__import__(name, fromlist=...)` with a non-empty `fromlist`: the named
# module itself, and for a package each `fromlist` name that is not an
# attribute yet imported as a submodule (CPython's `_handle_fromlist`).
def _import_fromlist(name, fromlist, pkg):
    m = __import__(name)
    for part in name.split('.')[1:]:
        m = getattr(m, part)
    if pkg:
        _import_handle_fromlist(m, name, fromlist, False)
    return m


def _import_handle_fromlist(m, name, fromlist, recursive):
    for x in fromlist:
        if type(x) is not str and not isinstance(x, str):
            where = name + '.__all__' if recursive else "``from list''"
            raise TypeError(f'Item in {where} must be str, not {type(x).__name__}')
        if x == '*':
            if not recursive and hasattr(m, '__all__'):
                _import_handle_fromlist(m, name, m.__all__, True)
        elif not hasattr(m, x):
            sub = name + '.' + x
            try:
                __import__(sub)
            except ModuleNotFoundError as e:
                if e.name != sub:
                    raise


# `isinstance`/`issubclass` against nested tuples, left to right: a class
# whose metaclass checks, or a walk longer than the native one. Iterative,
# so any nesting depth works and fuel applies.
def _isinstance_any(x, classes):
    todo = [classes]
    while todo:
        c = todo.pop()
        if type(c) is tuple:
            todo.extend(reversed(c))
        elif isinstance(x, c):
            return True
    return False


def _issubclass_any(x, classes):
    todo = [classes]
    while todo:
        c = todo.pop()
        if type(c) is tuple:
            todo.extend(reversed(c))
        elif issubclass(x, c):
            return True
    return False


# ------------------------------------------------- GenericAlias, Union
# `list[int]` and `int | None` (CPython's Objects/genericaliasobject.c and
# unionobject.c). The engine builds them through `_generic_alias` and
# `_union_or`; `types` and `typing` export the classes.

_NoneType = type(None)


def _type_repr(p):
    if p is ...:
        return '...'
    if p is _NoneType:
        return 'None'
    if type(p) is list:
        return '[' + ', '.join([_type_repr(x) for x in p]) + ']'
    try:
        p.__origin__
        p.__args__
        return repr(p)
    except AttributeError:
        pass
    try:
        q = p.__qualname__
        m = p.__module__
    except AttributeError:
        return repr(p)
    if m is None:
        return repr(p)
    return q if m == 'builtins' else f'{m}.{q}'


def _no_setattr(self, name):
    raise AttributeError(f"'{_type_name(self)}' object has no attribute '{name}' "
                         'and no __dict__ for setting new attributes')


def _unionable(x):
    return x is None or isinstance(x, (type, GenericAlias, Union))


class GenericAlias:
    __module__ = 'types'
    __mm_tpname__ = 'types.GenericAlias'

    def __new__(cls, origin, args, /):
        self = object.__new__(cls)
        object.__setattr__(self, '__origin__', origin)
        object.__setattr__(self, '__args__', args if type(args) is tuple else (args,))
        object.__setattr__(self, '__parameters__', ())
        return self

    def __setattr__(self, name, value):
        _no_setattr(self, name)

    def __getattr__(self, name):
        if name in ('__copy__', '__deepcopy__', '__reduce_ex__', '__unpacked__',
                    '__typing_unpacked_tuple_args__'):
            raise AttributeError(name)
        return getattr(self.__origin__, name)

    def __repr__(self):
        args = self.__args__
        inner = ', '.join([_type_repr(a) for a in args]) if args else '()'
        return f'{_type_repr(self.__origin__)}[{inner}]'

    def __eq__(self, other):
        if not isinstance(other, GenericAlias):
            return NotImplemented
        return self.__origin__ == other.__origin__ and self.__args__ == other.__args__

    def __hash__(self):
        return hash(self.__origin__) ^ hash(self.__args__)

    def __call__(self, *args, **kwargs):
        obj = self.__origin__(*args, **kwargs)
        try:
            obj.__orig_class__ = self
        except (AttributeError, TypeError):
            pass
        return obj

    def __getitem__(self, item):
        raise TypeError(f'{self!r} is not a generic class')

    __iter__ = None

    def __mro_entries__(self, bases):
        return (self.__origin__,)

    def __instancecheck__(self, obj):
        raise TypeError('isinstance() argument 2 cannot be a parameterized generic')

    def __subclasscheck__(self, cls):
        raise TypeError('issubclass() argument 2 cannot be a parameterized generic')

    def __reduce__(self):
        return GenericAlias, (self.__origin__, self.__args__)

    def __or__(self, other):
        if not _unionable(other):
            return NotImplemented
        return _union_of((self, other))

    def __ror__(self, other):
        if not _unionable(other):
            return NotImplemented
        return _union_of((other, self))


def _union_of(args):
    # Flattened, `None` as `NoneType`, duplicates dropped; one member is
    # the member itself.
    flat = []
    for a in args:
        for x in (a.__args__ if type(a) is Union else (a,)):
            if x is None:
                x = _NoneType
            for y in flat:
                if y is x or (type(y) is type(x) and y == x):
                    break
            else:
                flat.append(x)
    if len(flat) == 1:
        return flat[0]
    self = object.__new__(Union)
    object.__setattr__(self, '__args__', tuple(flat))
    return self


class Union:
    __module__ = 'typing'
    __mm_tpname__ = 'typing.Union'

    def __new__(cls, *args, **kwargs):
        raise TypeError("cannot create 'typing.Union' instances")

    def __class_getitem__(cls, params):
        if type(params) is not tuple:
            params = (params,)
        if not params:
            raise TypeError('Cannot take a Union of no types.')
        return _union_of(params)

    def __setattr__(self, name, value):
        _no_setattr(self, name)

    @property
    def __origin__(self):
        return Union

    @property
    def __parameters__(self):
        return ()

    def __repr__(self):
        return ' | '.join([_type_repr(a) for a in self.__args__])

    def __getitem__(self, item):
        raise TypeError(f'{self!r} is not a generic class')

    __iter__ = None

    def __eq__(self, other):
        if type(other) is not Union:
            return NotImplemented
        try:
            return set(self.__args__) == set(other.__args__)
        except TypeError:
            a, b = self.__args__, other.__args__
            return all(x in b for x in a) and all(y in a for y in b)

    def __hash__(self):
        # An unhashable argument raises its own TypeError, as CPython's
        # union_hash does, not frozenset's "cannot use ... as a set element".
        for a in self.__args__:
            hash(a)
        return hash(frozenset(self.__args__))

    def __reduce__(self):
        return _union_of, (self.__args__,)

    def __or__(self, other):
        return _union_of((self, other))

    def __ror__(self, other):
        return _union_of((other, self))

    def __instancecheck__(self, obj):
        for a in self.__args__:
            if isinstance(obj, a):
                return True
        return False

    def __subclasscheck__(self, cls):
        for a in self.__args__:
            if issubclass(cls, a):
                return True
        return False

    def __mro_entries__(self, bases):
        raise TypeError(f'Cannot subclass {self!r}')


def _generic_alias(origin, args):
    return GenericAlias(origin, args)


def _union_or(a, b):
    return _union_of((a, b))


def _class_getitem(cls, *args):
    if len(args) != 1:
        raise TypeError(f'{cls.__name__}.__class_getitem__() takes exactly one argument '
                        f'({len(args)} given)')
    return GenericAlias(cls, args[0])


__name__ = 'builtins'


def globals():
    # Decision G2 (docs/spec.md §4): a read-only view of the calling
    # module's globals. Reads work as on CPython's dict; writes raise.
    import _mm
    c = _mm.caller(1, ())
    return _GlobalsView(c[2] if c is not None else {})


def _globals_ro(*args):
    raise TypeError('micro-monty: globals() is a read-only view; '
                    'assign or del the name itself (or use a dict of your own)')


class _GlobalsView:

    def __init__(self, g):
        self._g = g

    def __getitem__(self, key):
        return self._g[key]

    def __contains__(self, key):
        return key in self._g

    def __iter__(self):
        return iter(self._g)

    def __len__(self):
        return len(self._g)

    def __repr__(self):
        return repr(self._g)

    def __eq__(self, other):
        if type(other) is _GlobalsView:
            other = other._g
        return self._g == other

    def __or__(self, other):
        return dict(self._g) | other

    def __ror__(self, other):
        return other | dict(self._g)

    def __reversed__(self):
        return reversed(self._g)

    def get(self, *args):
        return self._g.get(*args)

    def keys(self):
        return self._g.keys()

    def values(self):
        return self._g.values()

    def items(self):
        return self._g.items()

    def copy(self):
        return dict(self._g)

    def __setitem__(self, *args):
        _globals_ro()

    def __delitem__(self, *args):
        _globals_ro()

    def __ior__(self, *args):
        _globals_ro()

    def update(self, *args, **kwargs):
        _globals_ro()

    def setdefault(self, *args):
        _globals_ro()

    def pop(self, *args):
        _globals_ro()

    def popitem(self):
        _globals_ro()

    def clear(self):
        _globals_ro()


class coroutine_wrapper:
    # What `coroutine.__await__()` returns: an iterator over the
    # coroutine's yields.
    __mm_tpname__ = 'coroutine_wrapper'

    def __init__(self, coro):
        self._coro = coro

    def __iter__(self):
        return self

    def __next__(self):
        return self._coro.send(None)

    def send(self, value):
        return self._coro.send(value)

    def throw(self, *args):
        return self._coro.throw(*args)

    def close(self):
        return self._coro.close()

    def __repr__(self):
        return f'<coroutine_wrapper object at {hex(id(self))}>'


def _eg_new(cls, args, kwargs):
    if kwargs:
        raise TypeError(f'{cls.__name__}() takes no keyword arguments')
    if len(args) != 2:
        raise TypeError(f'BaseExceptionGroup.__new__() takes exactly 2 arguments ({len(args)} given)')
    message, excs = args
    if not isinstance(message, str):
        raise TypeError(f'BaseExceptionGroup.__new__() argument 1 must be str, not {type(message).__name__}')
    if isinstance(excs, (set, frozenset, dict)) or not hasattr(excs, '__getitem__'):
        raise TypeError('second argument (exceptions) must be a sequence')
    seq = tuple(excs)
    if not seq:
        raise ValueError('second argument (exceptions) must be a non-empty sequence')
    plain = True
    for i, e in enumerate(seq):
        if not isinstance(e, BaseException):
            raise ValueError(f'Item {i} of second argument (exceptions) is not an exception')
        if not isinstance(e, Exception):
            plain = False
    if cls is BaseExceptionGroup:
        if plain:
            cls = ExceptionGroup
    elif issubclass(cls, Exception) and not plain:
        if cls is ExceptionGroup:
            raise TypeError('Cannot nest BaseExceptions in an ExceptionGroup')
        raise TypeError(f'Cannot nest BaseExceptions in {cls.__name__!r}')
    self = BaseException.__new__(cls, message, excs)
    self._mm_message = message
    self._mm_excs = seq
    return self


def _eg_matcher(cond):
    if isinstance(cond, type) and issubclass(cond, BaseException):
        return lambda e: isinstance(e, cond)
    if isinstance(cond, tuple) and all(isinstance(c, type) and issubclass(c, BaseException) for c in cond):
        return lambda e: isinstance(e, cond)
    if callable(cond) and not isinstance(cond, type):
        return cond
    raise TypeError('expected an exception type, a tuple of exception types, or a callable (other than a class)')


def _eg_derive(eg, excs):
    g = eg.derive(excs)
    g.__traceback__ = eg.__traceback__
    g.__cause__ = eg.__cause__
    g.__context__ = eg.__context__
    g.__suppress_context__ = eg.__suppress_context__
    notes = getattr(eg, '__notes__', None)
    if notes is not None:
        g.__notes__ = list(notes)
    return g


def _eg_split(eg, match):
    if match(eg):
        return eg, None
    yes = []
    no = []
    for e in eg.exceptions:
        if isinstance(e, BaseExceptionGroup):
            a, b = _eg_split(e, match)
            if a is not None:
                yes.append(a)
            if b is not None:
                no.append(b)
        elif match(e):
            yes.append(e)
        else:
            no.append(e)
    return (_eg_derive(eg, yes) if yes else None), (_eg_derive(eg, no) if no else None)


def _star_match(st, typ):
    # except*'s CHECK_EG_MATCH over the state [orig, rest, raised...]:
    # the matching part of the rest (a naked exception wrapped in a group
    # whose traceback is this frame), which leaves the rest in st[1].
    import _mm
    items = typ if type(typ) is tuple else (typ,)
    for t in items:
        if not (isinstance(t, type) and issubclass(t, BaseException)):
            raise TypeError('catching classes that do not inherit from BaseException is not allowed')
    for t in items:
        if issubclass(t, BaseExceptionGroup):
            raise TypeError('catching ExceptionGroup with except* is not allowed. Use except instead.')
    rest = st[1]
    if rest is None:
        return None
    if isinstance(rest, typ):
        if isinstance(rest, BaseExceptionGroup):
            match = rest
        else:
            match = BaseExceptionGroup('', (rest,))
            _mm.exc_tb_here(match)
        st[1] = None
        return match
    if isinstance(rest, BaseExceptionGroup):
        pair = rest.split(typ)
        if type(pair) is not tuple:
            raise TypeError(f'{type(rest).__name__}.split must return a tuple, not {type(pair).__name__}')
        if len(pair) < 2:
            raise TypeError(f'{type(rest).__name__}.split must return a 2-tuple, got tuple of size {len(pair)}')
        st[1] = pair[1]
        return pair[0]
    return None


def _star_leaves(e, ids):
    if isinstance(e, BaseExceptionGroup):
        for x in e.exceptions:
            _star_leaves(x, ids)
    else:
        ids.add(id(e))


def _star_prep(st):
    # _PyExc_PrepReraiseStar: re-raised parts keep their place in the
    # original group; new exceptions join them in a new group.
    import _mm
    orig = st[0]
    excs = st[2:]
    excs.append(st[1])
    if not isinstance(orig, BaseExceptionGroup):
        return excs[0]
    raised = []
    ids = set()
    for e in excs:
        if e is None:
            continue
        if _mm.exc_same_meta(e, orig):
            _star_leaves(e, ids)
        else:
            raised.append(e)
    kept = _eg_split(orig, lambda x: id(x) in ids)[0] if ids else None
    if not raised:
        return kept
    if kept is not None:
        raised.append(kept)
    if len(raised) > 1:
        return BaseExceptionGroup('', raised)
    return raised[0]


class BaseExceptionGroup(BaseException):

    def __new__(cls, *args, **kwargs):
        return _eg_new(cls, args, kwargs)

    def __init__(self, *args, **kwargs):
        BaseException.__init__(self, *args)

    @property
    def message(self):
        return self._mm_message

    @property
    def exceptions(self):
        return self._mm_excs

    def __str__(self):
        n = len(self._mm_excs)
        return f"{self._mm_message} ({n} sub-exception{'s' if n > 1 else ''})"

    def derive(self, excs):
        return BaseExceptionGroup(self._mm_message, excs)

    def split(self, condition):
        return _eg_split(self, _eg_matcher(condition))

    def subgroup(self, condition):
        return _eg_split(self, _eg_matcher(condition))[0]


class ExceptionGroup(BaseExceptionGroup, Exception):
    __module__ = 'builtins'


def _bulk(r, prog, pc, i, res):
    # A chunked set or dict operation (`Vm::bulk`) from item `i` of phase
    # `pc` on, once an item needs Python code: each phase is 16 bits of
    # `prog` (action, then the out, source and other registers of `r`;
    # `set::phase`), the value register `res`, or 6 for None, 7 for True.
    # The phase `pc` names has begun natively; a later one begins here.
    fresh = False
    while True:
        p = (prog >> (16 * pc)) & 0xFFFF
        if not p:
            break
        a = p & 15
        out = r[(p >> 4) & 7]
        src = r[(p >> 7) & 7]
        other = r[(p >> 10) & 7]
        if fresh and a == 3 and not p & 24576 and (
                type(src) is set or type(src) is frozenset or type(src) is dict):
            # A set or dict (not a view) presizes `out` (`set_merge`).
            _set_table(out, (src,))
        fresh = True
        if a == 10:
            _set_table(out, src)
            i = 0
            pc += 1
            continue
        while True:
            t = _more(src, i, p & 8192, None)
            if t is None:
                break
            at, x, v, h = t
            i = at + 1
            if a == 6 and type(src) is dict:
                # An intersection hashes a dict's keys again (`Vm::bulk`).
                h = None
            if a <= 2:
                if a == 1:
                    pair = _pair_list(x)
                    if len(pair) != 2:
                        raise ValueError(f"dictionary update sequence element #{at} has length {len(pair)}; 2 is required")
                    x, v = pair
                    h = None
                elif a == 2:
                    v = other
                if h is None:
                    h = _khash(x, 'dict key')
                e = _dfind(out, x, h)
                if e < 0:
                    _dict_insert(out, x, h, v)
                else:
                    _dict_set_at(out, e, v)
                continue
            if h is None:
                # An intersection hashes as `hash()` does (`set_intersection`).
                h = hash(x) if a == 6 and type(other) is not dict else _khash(x, 'set element')
            if a == 3:
                _sadd_h(out, x, h)
            elif a == 4 or a == 5:
                j = _sfind(out, x, h)
                if j >= 0:
                    _set_table(out, j)
                elif a == 5:
                    _set_insert(out, x, h)
            else:
                if type(other) is dict:
                    has = _dfind(other, x, h) >= 0
                else:
                    has = _sfind(other, x, h) >= 0
                if a == 6 and has or a == 7 and not has:
                    _sadd_h(out, x, h)
                    # Holding all of the set it probes, an intersection
                    # reads no further (`Item::End`).
                    if a == 6 and type(other) is not dict and len(out) >= len(other):
                        break
                elif a == 8 and not has or a == 9 and has:
                    return False
        if a == 4:
            _set_table(out, out)
        i = 0
        pc += 1
    if res == 6:
        return None
    if res == 7:
        return True
    return r[res]


def _seq_scan_rest(s, x, i, stop, kind, c):
    # `count` (`kind` 0), `index` (1) or `remove` (2) from item `i` on,
    # once an item's `==` needs Python code (`Vm::seq_scan`), or over a
    # range for an `x` that is not an int (`range_search`).
    while i < stop and i < len(s):
        y = s[i]
        if y is x or y == x:
            if kind == 0:
                c += 1
            elif kind == 1:
                return i
            else:
                del s[i]
                return None
        i += 1
    if kind == 0:
        return c
    if kind == 1:
        t = _type_name(s)
        if t == 'range':
            # CPython's iterating search, for an `x` that is not an int.
            raise ValueError('sequence.index(x): x not in sequence')
        raise ValueError(f'{t}.index(x): x not in {t}')
    raise ValueError('list.remove(x): x not in list')
