# zlib: only the checksums, crc32 and adler32 (native, in mm-engine's
# binascii.rs), with CPython's argument handling and messages. There is no
# deflate: compress, decompress and the rest raise AttributeError
# (docs/user-guide.md Limitations).

from binascii import _crc32, _adler32, _buffer, _uint


class error(Exception):
    pass


error.__module__ = 'zlib'


def _args(name, args, kwargs):
    if kwargs:
        raise TypeError(f'zlib.{name}() takes no keyword arguments')
    if not args:
        raise TypeError(f'{name} expected at least 1 argument, got 0')
    if len(args) > 2:
        raise TypeError(f'{name} expected at most 2 arguments, got {len(args)}')
    return _buffer(args[0]), args[1] if len(args) > 1 else None


def crc32(*args, **kwargs):
    data, value = _args('crc32', args, kwargs)
    return _crc32(data, 0 if value is None else _uint(value))


def adler32(*args, **kwargs):
    data, value = _args('adler32', args, kwargs)
    return _adler32(data, 1 if value is None else _uint(value))


_ABSENT = ('compress', 'decompress', 'compressobj', 'decompressobj', '_ZlibDecompressor')


def __getattr__(name):
    if name in _ABSENT:
        raise AttributeError(f"module 'zlib' has no attribute '{name}' "
                             '(micro-monty bundles only zlib.crc32 and zlib.adler32)')
    raise AttributeError(f"module 'zlib' has no attribute '{name}'")
