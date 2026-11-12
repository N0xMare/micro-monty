# hashlib: the digests are native (mm-engine's hashlib.rs); this module
# holds CPython 3.14's surface (`_hashlib.HASH`, `_hashlib.HASHXOF`,
# `_blake2.blake2b/2s`) and its argument checks and messages
# (docs/spec.md §12).

from operator import index as _index

_MISSING = object()

# name: (algorithm number, digest size, block size)
_ALGS = {
    'md5': (0, 16, 64),
    'sha1': (1, 20, 64),
    'sha224': (2, 28, 64),
    'sha256': (3, 32, 64),
    'sha384': (4, 48, 128),
    'sha512': (5, 64, 128),
    'sha3_224': (6, 28, 144),
    'sha3_256': (7, 32, 136),
    'sha3_384': (8, 48, 104),
    'sha3_512': (9, 64, 72),
    'shake_128': (10, 0, 168),
    'shake_256': (11, 0, 136),
    'blake2b': (12, 64, 128),
    'blake2s': (13, 32, 64),
}

# OpenSSL's names, matched without regard to case.
_OPENSSL = {
    'md5': 'md5', 'ssl3-md5': 'md5', 'sha1': 'sha1', 'sha-1': 'sha1', 'ssl3-sha1': 'sha1',
    'sha224': 'sha224', 'sha-224': 'sha224', 'sha2-224': 'sha224',
    'sha256': 'sha256', 'sha-256': 'sha256', 'sha2-256': 'sha256',
    'sha384': 'sha384', 'sha-384': 'sha384', 'sha2-384': 'sha384',
    'sha512': 'sha512', 'sha-512': 'sha512', 'sha2-512': 'sha512',
    'sha3-224': 'sha3_224', 'sha3-256': 'sha3_256', 'sha3-384': 'sha3_384',
    'sha3-512': 'sha3_512', 'shake128': 'shake_128', 'shake256': 'shake_256',
    'blake2b512': 'blake2b', 'blake2s256': 'blake2s',
}

_DATA_STRING = ("'data' and 'string' are mutually exclusive and support for 'string' keyword "
                'parameter is slated for removal in a future version.')

algorithms_guaranteed = {'md5', 'sha1', 'sha224', 'sha256', 'sha384', 'sha512', 'sha3_224',
                         'sha3_256', 'sha3_384', 'sha3_512', 'shake_128', 'shake_256',
                         'blake2b', 'blake2s'}
algorithms_available = set(algorithms_guaranteed)

__all__ = ('md5', 'sha1', 'sha224', 'sha256', 'sha384', 'sha512', 'blake2b', 'blake2s',
           'sha3_224', 'sha3_256', 'sha3_384', 'sha3_512', 'shake_128', 'shake_256',
           'new', 'algorithms_guaranteed', 'algorithms_available', 'pbkdf2_hmac', 'file_digest')


def _clinic(fname, args, kwargs, names, maxpos, minpos):
    # Argument Clinic's `_PyArg_UnpackKeywords`, with its messages.
    nargs = len(args)
    nkw = len(kwargs)
    maxargs = len(names)
    if nargs + nkw > maxargs:
        raise TypeError(f"{fname}() takes at most {maxargs} {'keyword ' if nargs == 0 else ''}"
                        f"argument{'' if maxargs == 1 else 's'} ({nargs + nkw} given)")
    if nargs > maxpos:
        if maxpos == 0:
            raise TypeError(f'{fname}() takes no positional arguments')
        raise TypeError(f"{fname}() takes {'at most' if minpos < maxpos else 'exactly'} {maxpos} "
                        f"positional argument{'' if maxpos == 1 else 's'} ({nargs} given)")
    out = list(args) + [_MISSING] * (maxargs - nargs)
    left = nkw
    for i in range(nargs, maxargs):
        if left:
            if names[i] in kwargs:
                out[i] = kwargs[names[i]]
                left -= 1
        elif i >= minpos:
            break
        if out[i] is _MISSING and i < minpos:
            raise TypeError(f"{fname}() missing required argument '{names[i]}' (pos {i + 1})")
    if left:
        for i in range(nargs):
            if names[i] in kwargs:
                raise TypeError(f"argument for {fname}() given by name ('{names[i]}') and "
                                f'position ({i + 1})')
        for k in kwargs:
            if k not in names:
                raise TypeError(f"{fname}() got an unexpected keyword argument '{k}'")
    return out


def _data(data, string):
    # `_Py_hashlib_data_argument`: the one data argument, or `_MISSING`.
    if data is not _MISSING and string is not _MISSING:
        raise TypeError(_DATA_STRING)
    return string if data is _MISSING else data


def _buffer(data):
    if isinstance(data, str):
        raise TypeError('Strings must be encoded before hashing')
    if not isinstance(data, bytes):
        raise TypeError('object supporting the buffer API required')
    return data


def _bytes_like(x):
    if not isinstance(x, bytes):
        raise TypeError(f"a bytes-like object is required, not '{type(x).__name__}'")
    return x


def _no_kw(fname, kwargs):
    if kwargs:
        raise TypeError(f'{fname}() takes no keyword arguments')


def _no_args(fname, args, kwargs):
    _no_kw(fname, kwargs)
    if args:
        raise TypeError(f'{fname}() takes no arguments ({len(args)} given)')


def _one_arg(fname, args, kwargs):
    _no_kw(fname, kwargs)
    if len(args) != 1:
        raise TypeError(f'{fname}() takes exactly one argument ({len(args)} given)')
    return _buffer(args[0])


def _readonly_setattr(self, name, value):
    t = type(self).__mm_tpname__
    if name in ('name', 'digest_size', 'block_size'):
        raise AttributeError(f"attribute '{name}' of '{t}' objects is not writable")
    raise AttributeError(f"'{t}' object has no attribute '{name}' and no __dict__ for setting "
                         'new attributes')


class HASH:
    __mm_tpname__ = '_hashlib.HASH'

    def __new__(cls, *args, **kwargs):
        raise TypeError(f"cannot create '{cls.__mm_tpname__}' instances")

    __setattr__ = _readonly_setattr

    @property
    def name(self):
        return self._name

    @property
    def digest_size(self):
        return _ALGS[self._name][1]

    @property
    def block_size(self):
        return _ALGS[self._name][2]

    def __repr__(self):
        return f'<{self._name} {type(self).__mm_tpname__} object @ {hex(id(self))}>'

    def update(self, *args, **kwargs):
        _h_update(self._st, _one_arg('HASH.update', args, kwargs))

    def digest(self, *args, **kwargs):
        _no_args('HASH.digest', args, kwargs)
        return _h_digest(self._st, 0)

    def hexdigest(self, *args, **kwargs):
        _no_args('HASH.hexdigest', args, kwargs)
        return _h_digest(self._st, 0).hex()

    def copy(self, *args, **kwargs):
        _no_args('HASH.copy', args, kwargs)
        return _wrap(type(self), self._name, _h_copy(self._st))


HASH.__module__ = '_hashlib'


def _length(fname, args, kwargs):
    (length,) = _clinic(fname, args, kwargs, ('length',), 1, 1)
    return _index(length)


class HASHXOF(HASH):
    __mm_tpname__ = '_hashlib.HASHXOF'

    def digest(self, *args, **kwargs):
        return _h_digest(self._st, _length('digest', args, kwargs))

    def hexdigest(self, *args, **kwargs):
        return _h_digest(self._st, _length('hexdigest', args, kwargs)).hex()


HASHXOF.__module__ = '_hashlib'


def _wrap(cls, name, st):
    h = object.__new__(cls)
    object.__setattr__(h, '_name', name)
    object.__setattr__(h, '_st', st)
    return h


def _openssl(name, data):
    # A `_hashlib` object for algorithm `name`, fed `data` (`_MISSING` or
    # checked bytes).
    st = _h_new(_ALGS[name][0])
    if data is not _MISSING:
        _h_update(st, data)
    return _wrap(HASHXOF if name.startswith('shake') else HASH, name, st)


def _constructor(name):
    fname = 'openssl_' + name

    def construct(*args, **kwargs):
        data, _usedforsecurity, string = _clinic(
            fname, args, kwargs, ('data', 'usedforsecurity', 'string'), 1, 0)
        data = _data(data, string)
        if data is not _MISSING:
            _buffer(data)
        return _openssl(name, data)

    construct.__name__ = fname
    construct.__qualname__ = fname
    return construct


md5 = _constructor('md5')
sha1 = _constructor('sha1')
sha224 = _constructor('sha224')
sha256 = _constructor('sha256')
sha384 = _constructor('sha384')
sha512 = _constructor('sha512')
sha3_224 = _constructor('sha3_224')
sha3_256 = _constructor('sha3_256')
sha3_384 = _constructor('sha3_384')
sha3_512 = _constructor('sha3_512')
shake_128 = _constructor('shake_128')
shake_256 = _constructor('shake_256')

_B2_NAMES = ('data', 'digest_size', 'key', 'salt', 'person', 'fanout', 'depth', 'leaf_size',
             'node_offset', 'node_depth', 'inner_size', 'last_node', 'usedforsecurity', 'string')


def _c_int(x):
    x = _index(x)
    if x > 0x7FFFFFFF or x < -0x80000000:
        raise OverflowError('Python int too large to convert to C int')
    return x


def _c_unsigned(x, what):
    x = _index(x)
    if x < 0:
        raise ValueError('Cannot convert negative int')
    if x > 0xFFFFFFFFFFFFFFFF:
        raise OverflowError(f'Python int too large for C {what}')
    return x


class _Blake2:
    def __setattr__(self, name, value):
        _readonly_setattr(self, name, value)

    def __init__(self, *args, **kwargs):
        cls = type(self)
        fname = cls.__name__
        big = fname == 'blake2b'
        (data, ds, key, salt, person, fanout, depth, leaf, offset, ndepth, inner, last,
         _usedforsecurity, string) = _clinic(fname, args, kwargs, _B2_NAMES, 1, 0)
        # Conversions in signature order, then the range checks.
        ds = cls.MAX_DIGEST_SIZE if ds is _MISSING else _c_int(ds)
        key = b'' if key is _MISSING else _bytes_like(key)
        salt = b'' if salt is _MISSING else _bytes_like(salt)
        person = b'' if person is _MISSING else _bytes_like(person)
        fanout = 1 if fanout is _MISSING else _c_int(fanout)
        depth = 1 if depth is _MISSING else _c_int(depth)
        leaf = 0 if leaf is _MISSING else _c_unsigned(leaf, 'unsigned long')
        offset = 0 if offset is _MISSING else _c_unsigned(offset, 'unsigned long long')
        ndepth = 0 if ndepth is _MISSING else _c_int(ndepth)
        inner = 0 if inner is _MISSING else _c_int(inner)
        last = False if last is _MISSING else bool(last)
        data = _data(data, string)
        size = cls.MAX_DIGEST_SIZE
        if not 1 <= ds <= size:
            raise ValueError(f"digest_size for Blake2{'b' if big else 's'} must be between 1 "
                             f'and {size} bytes, here it is {ds}')
        if len(salt) > cls.SALT_SIZE:
            raise ValueError(f'maximum salt length is {cls.SALT_SIZE} bytes')
        if len(person) > cls.PERSON_SIZE:
            raise ValueError(f'maximum person length is {cls.PERSON_SIZE} bytes')
        if not 0 <= fanout <= 255:
            raise ValueError('fanout must be between 0 and 255')
        if not 1 <= depth <= 255:
            raise ValueError('depth must be between 1 and 255')
        if leaf > 0xFFFFFFFF:
            raise OverflowError('leaf_size is too large')
        if not big and offset > 0xFFFFFFFFFFFF:
            raise OverflowError('node_offset is too large')
        if not 0 <= ndepth <= 255:
            raise ValueError('node_depth must be between 0 and 255')
        if not 0 <= inner <= size:
            raise ValueError(f'inner_size must be between 0 and is {size}')
        if len(key) > cls.MAX_KEY_SIZE:
            raise ValueError(f'maximum key length is {cls.MAX_KEY_SIZE} bytes')
        if data is not _MISSING:
            _buffer(data)
        ss = cls.SALT_SIZE
        params = (bytes([ds, len(key), fanout, depth]) + leaf.to_bytes(4, 'little')
                  + offset.to_bytes(8 if big else 6, 'little') + bytes([ndepth, inner])
                  + bytes(14 if big else 0) + salt + bytes(ss - len(salt))
                  + person + bytes(ss - len(person)))
        st = _h_new(12 if big else 13, params, last)
        if key:
            _h_update(st, key + bytes(2 * size - len(key)))
        if data is not _MISSING:
            _h_update(st, data)
        object.__setattr__(self, '_st', st)
        object.__setattr__(self, '_ds', ds)

    @property
    def name(self):
        return type(self).__name__

    @property
    def digest_size(self):
        return self._ds

    @property
    def block_size(self):
        return 2 * type(self).MAX_DIGEST_SIZE

    def update(self, *args, **kwargs):
        _h_update(self._st, _one_arg(f'{type(self).__name__}.update', args, kwargs))

    def digest(self, *args, **kwargs):
        _no_args(f'{type(self).__name__}.digest', args, kwargs)
        return _h_digest(self._st, 0)

    def hexdigest(self, *args, **kwargs):
        _no_args(f'{type(self).__name__}.hexdigest', args, kwargs)
        return _h_digest(self._st, 0).hex()

    def copy(self, *args, **kwargs):
        cls = type(self)
        _no_args(f'{cls.__name__}.copy', args, kwargs)
        h = object.__new__(cls)
        object.__setattr__(h, '_st', _h_copy(self._st))
        object.__setattr__(h, '_ds', self._ds)
        return h


class blake2b(_Blake2):
    __mm_tpname__ = '_blake2.blake2b'
    MAX_DIGEST_SIZE = 64
    MAX_KEY_SIZE = 64
    SALT_SIZE = 16
    PERSON_SIZE = 16


class blake2s(_Blake2):
    __mm_tpname__ = '_blake2.blake2s'
    MAX_DIGEST_SIZE = 32
    MAX_KEY_SIZE = 32
    SALT_SIZE = 8
    PERSON_SIZE = 8


blake2b.__module__ = '_blake2'
blake2s.__module__ = '_blake2'


def _lookup(name):
    # `_hashlib`'s digest lookup: Python's names exactly, OpenSSL's in any case.
    if name in _ALGS:
        return name
    return _OPENSSL.get(name.lower())


def _builtin_constructor(name):
    if name in ('blake2b', 'blake2s'):
        return blake2b if name == 'blake2b' else blake2s
    alg = name.lower() if name in ('SHA1', 'MD5', 'SHA224', 'SHA256', 'SHA384', 'SHA512') else name
    if alg in _ALGS:
        return globals()[alg]
    raise ValueError('unsupported hash type ' + name)


def __hash_new(name, *args, **kwargs):
    """new(name, data=b'') - Return a new hashing object using the named algorithm;
    optionally initialized with data (which must be a bytes-like object).
    """
    if name in ('blake2b', 'blake2s'):
        return _builtin_constructor(name)(*args, **kwargs)
    _name, data, _usedforsecurity, string = _clinic(
        'new', (name,) + args, kwargs, ('name', 'data', 'usedforsecurity', 'string'), 2, 1)
    if not isinstance(name, str):
        raise TypeError(f"new() argument 'name' must be str, not {type(name).__name__}")
    data = _data(data, string)
    if data is not _MISSING:
        _buffer(data)
    alg = _lookup(name)
    if alg is None:
        return _builtin_constructor(name)(*args, **kwargs)
    return _openssl(alg, data)


new = __hash_new


def pbkdf2_hmac(*args, **kwargs):
    hash_name, password, salt, iterations, dklen = _clinic(
        'pbkdf2_hmac', args, kwargs, ('hash_name', 'password', 'salt', 'iterations', 'dklen'), 5, 4)
    if not isinstance(hash_name, str):
        raise TypeError("pbkdf2_hmac() argument 'hash_name' must be str, "
                        f'not {type(hash_name).__name__}')
    password = _bytes_like(password)
    salt = _bytes_like(salt)
    iterations = _index(iterations)
    if iterations > 0x7FFFFFFFFFFFFFFF or iterations < -0x8000000000000000:
        raise OverflowError('Python int too large to convert to C long')
    alg = _lookup(hash_name)
    if alg is None:
        raise ValueError('unsupported hash type ' + hash_name)
    if iterations < 1:
        raise ValueError('iteration value must be greater than 0.')
    if iterations > 0x7FFFFFFF:
        raise OverflowError('iteration value is too great.')
    num, size, block = _ALGS[alg]
    if dklen is _MISSING or dklen is None:
        dklen = size
    else:
        dklen = _index(dklen)
        if dklen > 0x7FFFFFFFFFFFFFFF or dklen < -0x8000000000000000:
            raise OverflowError('Python int too large to convert to C long')
    if dklen < 1:
        raise ValueError('key length must be greater than 0.')
    if dklen > 0x7FFFFFFF:
        raise OverflowError('key length is too great.')
    if not size:
        raise ValueError(f'unsupported digest for PBKDF2: {alg}')
    if len(password) > block:
        st = _h_new(num)
        _h_update(st, password)
        password = _h_digest(st, 0)
    password = password + bytes(block - len(password))
    inner = _h_new(num)
    _h_update(inner, bytes([b ^ 0x36 for b in password]))
    outer = _h_new(num)
    _h_update(outer, bytes([b ^ 0x5C for b in password]))
    out = b''
    i = 1
    while len(out) < dklen:
        st = _h_copy(inner)
        _h_update(st, salt + i.to_bytes(4, 'big'))
        o = _h_copy(outer)
        _h_update(o, _h_digest(st, 0))
        u = _h_digest(o, 0)
        acc = u
        left = iterations - 1
        while left > 0:
            n = 4096 if left > 4096 else left
            u, acc = _h_pbkdf2(inner, outer, u, acc, n)
            left -= n
        out += acc
        i += 1
    return out[:dklen]


def file_digest(fileobj, digest, /, *, _bufsize=2**18):
    """Hash the contents of a file-like object. Returns a digest object."""
    if isinstance(digest, str):
        digestobj = new(digest)
    else:
        digestobj = digest()
    while True:
        data = fileobj.read(_bufsize)
        if not data:
            break
        digestobj.update(data)
    return digestobj


del _constructor
