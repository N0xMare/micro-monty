# struct: the packing and unpacking are native (mm-engine's structmod.rs,
# from CPython 3.14's _struct.c); this module holds the API. Native sizes
# and alignment are x86-64 Linux's on every target, so results do not
# depend on where a snapshot resumes. pack_into writes into a bytearray
# (micro-monty has no memoryview) through slice assignment.

from operator import index as _index

__all__ = ['calcsize', 'pack', 'pack_into', 'unpack', 'unpack_from', 'iter_unpack', 'Struct', 'error']

_MISSING = object()


class error(Exception):
    __module__ = 'struct'


def _check(r):
    if type(r) is str:
        raise error(r)
    return r


def _format(fmt):
    # A non-ASCII str format fails as CPython's encode does.
    if type(fmt) is str and not fmt.isascii():
        fmt.encode('ascii')
    return fmt


def calcsize(format, /):
    return _check(_calcsize(_format(format)))


_CONVERT = ((_index, 'required argument is not an integer'),
            (float, 'required argument is not a float'),
            (bool, None),
            (complex, 'required argument is not a complex'))


def _pack_args(fmt, args):
    while True:
        r = _pack(fmt, args)
        if type(r) is not tuple:
            return _check(r)
        # A user object: convert it here (it may run Python), then retry.
        i, kind = r
        conv, msg = _CONVERT[kind]
        try:
            v = conv(args[i])
        except TypeError:
            if msg is None:
                raise
            raise error(msg) from None
        args = args[:i] + (v,) + args[i + 1:]


def pack(*args):
    if not args:
        raise TypeError('missing format argument')
    return _pack_args(_format(args[0]), args[1:])


def _pack_into(fmt, args):
    n = _check(_calclen(fmt))
    if len(args) != n + 2:
        if not args:
            raise error('pack_into expected buffer argument')
        if len(args) == 1:
            raise error('pack_into expected offset argument')
        raise error(f'pack_into expected {n} items for packing (got {len(args) - 2})')
    buffer = args[0]
    if type(buffer) is not bytearray:
        raise TypeError(f'argument must be read-write bytes-like object, not {type(buffer).__name__}')
    offset = _index(args[1])
    if offset >= 1 << 63 or offset < -(1 << 63):
        raise IndexError("cannot fit 'int' into an index-sized integer")
    size = _check(_calcsize(fmt))
    blen = len(buffer)
    if offset < 0:
        if offset + size > 0:
            raise error(f'no space to pack {size} bytes at offset {offset}')
        if offset + blen < 0:
            raise error(f'offset {offset} out of range for {blen}-byte buffer')
        offset += blen
    if blen - offset < size:
        raise error(f'pack_into requires a buffer of at least {size + offset} bytes for packing '
                    f'{size} bytes at offset {offset} (actual buffer size is {blen})')
    buffer[offset:offset + size] = _pack_args(fmt, args[2:])


def pack_into(*args):
    if not args:
        raise TypeError('missing format argument')
    return _pack_into(_format(args[0]), args[1:])


def unpack(format, buffer, /):
    return _check(_unpack(_format(format), buffer, 0, True))


def unpack_from(format, /, buffer, offset=0):
    return _check(_unpack(_format(format), buffer, _index(offset), False))


class _unpack_iterator:
    __mm_tpname__ = '_struct.unpack_iterator'

    def __init__(self, fmt, size, buffer):
        self._fmt = fmt
        self._size = size
        self._buffer = buffer
        self._offset = 0

    def __iter__(self):
        return self

    def __next__(self):
        if self._buffer is None or self._offset >= len(self._buffer):
            self._buffer = None
            raise StopIteration
        r = _unpack(self._fmt, self._buffer, self._offset, False)
        self._offset += self._size
        return r

    def __length_hint__(self):
        if self._buffer is None:
            return 0
        return (len(self._buffer) - self._offset) // self._size


def iter_unpack(format, buffer, /):
    fmt = _format(format)
    size = _check(_calcsize(fmt))
    if size == 0:
        raise error('cannot iteratively unpack with a struct of length 0')
    if not isinstance(buffer, (bytes, bytearray)):
        raise TypeError(f"a bytes-like object is required, not '{type(buffer).__name__}'")
    if len(buffer) % size:
        raise error(f'iterative unpacking requires a buffer of a multiple of {size} bytes')
    return _unpack_iterator(fmt, size, buffer)


class Struct:
    __module__ = '_struct'

    def __init__(self, format=_MISSING):
        if format is _MISSING:
            raise TypeError("Struct() missing required argument 'format' (pos 1)")
        self._fmt = _format(format)
        self.size = _check(_calcsize(self._fmt))
        self.format = format if type(format) is str else format.decode('ascii')

    def pack(self, *args):
        return _pack_args(self._fmt, args)

    def pack_into(self, *args):
        return _pack_into(self._fmt, args)

    def unpack(self, buffer, /):
        return _check(_unpack(self._fmt, buffer, 0, True))

    def unpack_from(self, buffer, offset=0):
        return _check(_unpack(self._fmt, buffer, _index(offset), False))

    def iter_unpack(self, buffer, /):
        return iter_unpack(self._fmt, buffer)

    def __repr__(self):
        return f'Struct({self.format!r})'
