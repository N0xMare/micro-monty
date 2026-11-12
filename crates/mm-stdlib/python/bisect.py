"""Bisection algorithms (adapted from CPython 3.14's Lib/bisect.py, with the
argument handling of its C accelerator _bisect)."""

_MAXSIZE = 9223372036854775807


def _own(cls, name):
    try:
        return cls.__dict__.get(name)
    except AttributeError:
        return None


def _tpname(obj):
    cls = type(obj)
    return _own(cls, '__mm_tpname__') or cls.__name__


def _index(x):
    if isinstance(x, int):
        return int(x)
    method = getattr(type(x), '__index__', None)
    if method is None:
        raise TypeError(f"'{_tpname(x)}' object cannot be interpreted as an integer")
    v = method(x)
    if not isinstance(v, int):
        raise TypeError(f'__index__ returned non-int (type {_tpname(v)})')
    return int(v)


def _bounds(a, lo, hi):
    lo = _index(lo)
    if lo > _MAXSIZE or lo < -_MAXSIZE - 1:
        raise OverflowError('Python int too large to convert to C ssize_t')
    if hi is not None:
        if not isinstance(hi, int) and getattr(type(hi), '__index__', None) is None:
            raise TypeError(f"argument should be integer or None, not '{_tpname(hi)}'")
        n = _index(hi)
        if n > _MAXSIZE or n < -_MAXSIZE - 1:
            raise OverflowError(f"cannot fit '{_tpname(hi)}' into an index-sized integer")
        hi = n
    if lo < 0:
        raise ValueError('lo must be non-negative')
    if hi is None or hi == -1:
        cls = type(a)
        if cls is dict or (isinstance(a, dict) and _own(cls, '__mm_tpname__') is not None):
            raise TypeError(f'{_tpname(a)} is not a sequence')
        hi = len(a)
    cls = type(a)
    if isinstance(a, (list, tuple, str, bytes, range)):
        pass
    elif isinstance(a, (dict, set)):
        # Builtin (C) mappings and sets have no item slot; Python subclasses
        # get one from their `__getitem__`.
        if cls is dict or cls is set or _own(cls, '__mm_tpname__') is not None:
            raise TypeError(f"'{_tpname(a)}' object does not support indexing")
    elif getattr(cls, '__getitem__', None) is None:
        raise TypeError(f"'{_tpname(a)}' object does not support indexing")
    return lo, hi


def insort_right(a, x, lo=0, hi=None, *, key=None):
    """Insert item x in list a, and keep it sorted assuming a is sorted."""
    if key is None:
        lo = bisect_right(a, x, lo, hi)
    else:
        lo = bisect_right(a, key(x), lo, hi, key=key)
    a.insert(lo, x)


def bisect_right(a, x, lo=0, hi=None, *, key=None):
    """Return the index where to insert item x in list a, assuming a is sorted."""
    lo, hi = _bounds(a, lo, hi)
    if key is None:
        while lo < hi:
            mid = (lo + hi) // 2
            if x < a[mid]:
                hi = mid
            else:
                lo = mid + 1
    else:
        while lo < hi:
            mid = (lo + hi) // 2
            if x < key(a[mid]):
                hi = mid
            else:
                lo = mid + 1
    return lo


def insort_left(a, x, lo=0, hi=None, *, key=None):
    """Insert item x in list a, and keep it sorted assuming a is sorted."""
    if key is None:
        lo = bisect_left(a, x, lo, hi)
    else:
        lo = bisect_left(a, key(x), lo, hi, key=key)
    a.insert(lo, x)


def bisect_left(a, x, lo=0, hi=None, *, key=None):
    """Return the index where to insert item x in list a, assuming a is sorted."""
    lo, hi = _bounds(a, lo, hi)
    if key is None:
        while lo < hi:
            mid = (lo + hi) // 2
            if a[mid] < x:
                lo = mid + 1
            else:
                hi = mid
    else:
        while lo < hi:
            mid = (lo + hi) // 2
            if key(a[mid]) < x:
                lo = mid + 1
            else:
                hi = mid
    return lo


# Create aliases
bisect = bisect_right
insort = insort_right
