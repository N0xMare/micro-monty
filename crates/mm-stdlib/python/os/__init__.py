# os: the sandbox has no ambient filesystem, environment or processes
# (docs/spec.md §1, §10). The pure parts are here (constants, fspath,
# os.path's string functions); every operation that touches the outside
# calls the host function the embedder registered under Monty's name for it
# (`os.getenv`, `os.environ`, `Path.read_text`, ...; `_mm.os_function`) and
# otherwise raises: PermissionError for the filesystem, RuntimeError for
# the environment and directory scans. Argument checks and messages follow
# CPython's posixmodule.c.

import _mm
from operator import index as _index

name = 'posix'
sep = '/'
altsep = None
extsep = '.'
curdir = '.'
pardir = '..'
pathsep = ':'
linesep = '\n'
defpath = '/bin:/usr/bin'
devnull = '/dev/null'
supports_bytes_environ = True
F_OK = 0
R_OK = 4
W_OK = 2
X_OK = 1
SEEK_SET = 0
SEEK_CUR = 1
SEEK_END = 2

_MISSING = object()

error = OSError


def _clinic(fname, args, kwargs, names, maxpos, minpos):
    # Argument Clinic's `_PyArg_UnpackKeywords`, with its messages.
    nargs = len(args)
    nkw = len(kwargs)
    maxargs = len(names)
    if nargs + nkw > maxargs:
        raise TypeError(f"{fname}() takes at most {maxargs} {'keyword ' if nargs == 0 else ''}"
                        f"argument{'' if maxargs == 1 else 's'} ({nargs + nkw} given)")
    if nargs > maxpos:
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


def _unavailable(op, kind):
    # The second sentence is the host's to replace (`HintGroup::OsHooks`);
    # the host-facing reason, a missing host function, is in docs/user-guide.md.
    hint = _mm.hint("os")
    raise kind(f"{op} is not available: this sandbox has no ambient "
               f"{'filesystem' if kind is PermissionError else 'operating system'}"
               + (". " + hint if hint else ""))


def _fs(op, *args):
    # A filesystem operation: the host's answer, or PermissionError.
    f = _mm.os_function(op)
    if f is None:
        _unavailable(op, PermissionError)
    return f(*args)


def _host(op, *args):
    # Any other OS operation: the host's answer, or RuntimeError.
    f = _mm.os_function(op)
    if f is None:
        _unavailable(op, RuntimeError)
    return f(*args)


class PathLike:
    """Abstract base class for implementing the file system path protocol."""

    def __fspath__(self):
        raise NotImplementedError


def _fspath(path):
    if isinstance(path, (str, bytes)):
        return path
    f = getattr(type(path), '__fspath__', None)
    if f is None:
        raise TypeError('expected str, bytes or os.PathLike object, not '
                        f'{type(path).__name__}')
    r = f(path)
    if not isinstance(r, (str, bytes)):
        raise TypeError(f'expected {type(path).__name__}.__fspath__() to return str or bytes, '
                        f'not {type(r).__name__}')
    return r


def fspath(*args, **kwargs):
    """Return the file system path representation of the object."""
    (path,) = _clinic('fspath', args, kwargs, ('path',), 1, 1)
    return _fspath(path)


def fsencode(filename):
    filename = _fspath(filename)
    return filename.encode() if isinstance(filename, str) else filename


def fsdecode(filename):
    filename = _fspath(filename)
    return filename.decode() if isinstance(filename, bytes) else filename


def _path_arg(fname, path, what, arg='path'):
    # posixmodule.c's path_converter: str, bytes, os.PathLike, or an fd.
    if isinstance(path, (str, bytes)) or hasattr(type(path), '__fspath__'):
        return _fspath(path)
    if 'integer' in what and isinstance(path, int):
        fd = _index(path)
        if fd > 0x7FFFFFFF:
            raise OverflowError('fd is greater than maximum')
        if fd < -0x80000000:
            raise OverflowError('fd is less than minimum')
        return fd
    raise TypeError(f'{fname}: {arg} should be {what}, not {type(path).__name__}')


def _fd_arg(fd):
    if fd is None or fd is _MISSING:
        return None
    fd = _index(fd)
    if fd > 0x7FFFFFFF:
        raise OverflowError('fd is greater than maximum')
    if fd < -0x80000000:
        raise OverflowError('fd is less than minimum')
    return fd


def _bad_fd(fd, with_name):
    if with_name:
        raise OSError(9, 'Bad file descriptor', fd)
    raise OSError(9, 'Bad file descriptor')


def _cint(x):
    x = _index(x)
    if not -0x80000000 <= x <= 0x7FFFFFFF:
        raise OverflowError('Python int too large to convert to C int')
    return x


def getenv(key, default=None):
    """Get an environment variable, return None if it doesn't exist."""
    if not isinstance(key, str):
        raise TypeError(f'str expected, not {type(key).__name__}')
    return _host('os.getenv', key, default)


def getenvb(key, default=None):
    if not isinstance(key, bytes):
        raise TypeError(f'bytes expected, not {type(key).__name__}')
    v = getenv(key.decode(), None)
    return default if v is None else v.encode()


def __getattr__(attr):
    # `os.environ` is read from the host on each access (PEP 562); `os.path`
    # is imported on first use (importing it during this module's body
    # would be circular).
    global path
    if attr == 'path':
        import os.path as p
        path = p
        return p
    if attr == 'environ':
        return dict(_host('os.environ'))
    if attr == 'environb':
        return {k.encode(): v.encode() for k, v in _host('os.environ').items()}
    raise AttributeError(f"module 'os' has no attribute '{attr}'")


def putenv(key, value):
    _host('os.putenv', key, value)


def unsetenv(key):
    _host('os.unsetenv', key)


def urandom(size):
    """Return a bytes object containing random bytes suitable for cryptographic use."""
    size = _index(size)
    if size < 0:
        raise ValueError('negative argument not allowed')
    f = _mm.os_function('os.urandom')
    if f is not None:
        return f(size)
    return _mm.urandom(size)


def getcwd():
    """Return a unicode string representing the current working directory."""
    f = _mm.os_function('os.getcwd')
    return '/' if f is None else f()


def getcwdb():
    """Return a bytes string representing the current working directory."""
    return getcwd().encode()


def getpid():
    return 1


def cpu_count():
    return None


def strerror(code):
    return {1: 'Operation not permitted', 2: 'No such file or directory',
            9: 'Bad file descriptor', 13: 'Permission denied', 17: 'File exists',
            20: 'Not a directory', 21: 'Is a directory', 39: 'Directory not empty'}.get(
                code, f'Unknown error {code}')


class stat_result(tuple):
    __mm_tpname__ = 'os.stat_result'
    n_sequence_fields = 10
    n_fields = 10

    def __new__(cls, seq):
        seq = tuple(seq)
        return tuple.__new__(cls, seq[:10])

    st_mode = property(lambda self: self[0])
    st_ino = property(lambda self: self[1])
    st_dev = property(lambda self: self[2])
    st_nlink = property(lambda self: self[3])
    st_uid = property(lambda self: self[4])
    st_gid = property(lambda self: self[5])
    st_size = property(lambda self: self[6])
    st_atime = property(lambda self: self[7])
    st_mtime = property(lambda self: self[8])
    st_ctime = property(lambda self: self[9])

    def __repr__(self):
        names = ('st_mode', 'st_ino', 'st_dev', 'st_nlink', 'st_uid', 'st_gid', 'st_size',
                 'st_atime', 'st_mtime', 'st_ctime')
        return 'os.stat_result(' + ', '.join([f'{n}={v!r}' for n, v in zip(names, self)]) + ')'


stat_result.__module__ = 'os'


def _stat_result(r):
    return r if isinstance(r, stat_result) else stat_result(r)


def stat(*args, **kwargs):
    """Perform a stat system call on the given path."""
    path, dir_fd, _follow = _clinic('stat', args, kwargs, ('path', 'dir_fd', 'follow_symlinks'),
                                    1, 1)
    path = _path_arg('stat', path, 'string, bytes, os.PathLike or integer')
    _fd_arg(dir_fd)
    if isinstance(path, int):
        _bad_fd(path, True)
    return _stat_result(_fs('Path.stat', _nonempty(fsdecode(path))))


def lstat(*args, **kwargs):
    path, dir_fd = _clinic('lstat', args, kwargs, ('path', 'dir_fd'), 1, 1)
    path = _path_arg('lstat', path, 'string, bytes or os.PathLike')
    _fd_arg(dir_fd)
    return _stat_result(_fs('Path.stat', _nonempty(fsdecode(path))))


def _nonempty(path):
    # The empty path names no file, whatever the host.
    if not path:
        raise FileNotFoundError(2, 'No such file or directory', path)
    return path


def listdir(*args, **kwargs):
    """Return a list containing the names of the files in the directory."""
    (path,) = _clinic('listdir', args, kwargs, ('path',), 1, 0)
    if path is _MISSING or path is None:
        path = '.'
    path = _path_arg('listdir', path, 'string, bytes, os.PathLike, integer or None')
    if isinstance(path, int):
        _bad_fd(path, False)
    names = [_basename(fsdecode(e)) for e in _fs('Path.iterdir', fsdecode(path))]
    return [n.encode() for n in names] if isinstance(path, bytes) else names


def _basename(p):
    return p[p.rfind('/') + 1:]


def mkdir(*args, **kwargs):
    """Create a directory."""
    path, mode, dir_fd = _clinic('mkdir', args, kwargs, ('path', 'mode', 'dir_fd'), 2, 1)
    path = _path_arg('mkdir', path, 'string, bytes or os.PathLike')
    if mode is not _MISSING:
        _cint(mode)
    _fd_arg(dir_fd)
    _fs('Path.mkdir', fsdecode(path), False, False)


def makedirs(name, mode=0o777, exist_ok=False):
    """Super-mkdir; create a leaf directory and all intermediate ones."""
    name = _fspath(name)
    _cint(mode)
    _fs('Path.mkdir', fsdecode(name), True, bool(exist_ok))


def _one_path(fname, args, kwargs):
    path, dir_fd = _clinic(fname, args, kwargs, ('path', 'dir_fd'), 1, 1)
    path = _path_arg(fname, path, 'string, bytes or os.PathLike')
    _fd_arg(dir_fd)
    return fsdecode(path)


def remove(*args, **kwargs):
    """Remove a file (same as unlink())."""
    _fs('Path.unlink', _one_path('remove', args, kwargs))


def unlink(*args, **kwargs):
    """Remove a file (same as remove())."""
    _fs('Path.unlink', _one_path('unlink', args, kwargs))


def rmdir(*args, **kwargs):
    """Remove a directory."""
    _fs('Path.rmdir', _one_path('rmdir', args, kwargs))


def _two_paths(fname, args, kwargs):
    src, dst, sfd, dfd = _clinic(fname, args, kwargs,
                                 ('src', 'dst', 'src_dir_fd', 'dst_dir_fd'), 2, 2)
    src = _path_arg(fname, src, 'string, bytes or os.PathLike', 'src')
    dst = _path_arg(fname, dst, 'string, bytes or os.PathLike', 'dst')
    _fd_arg(sfd)
    _fd_arg(dfd)
    return fsdecode(src), fsdecode(dst)


def rename(*args, **kwargs):
    """Rename a file or directory."""
    _fs('Path.rename', *_two_paths('rename', args, kwargs))


def replace(*args, **kwargs):
    """Rename a file or directory, overwriting the destination."""
    _fs('Path.rename', *_two_paths('replace', args, kwargs))


def scandir(path='.'):
    """Return an iterator of DirEntry objects for given path."""
    path = _path_arg('scandir', path, 'string, bytes, os.PathLike, integer or None')
    # The host answers the entries (pathlib's `Path.scan` protocol).
    return iter(_host('Path.scan', fsdecode(path), 'scandir', None))


def walk(top, topdown=True, onerror=None, followlinks=False):
    """Directory tree generator."""
    top = fspath(top)
    return iter([(d, list(ds), list(fs)) for d, ds, fs in
                 _host('Path.scan', fsdecode(top), 'walk', None)])


def access(path, mode):
    return _fs('Path.exists', fsdecode(_fspath(path)))


def chdir(path):
    _host('os.chdir', fsdecode(_fspath(path)))


def system(command):
    _host('os.system', command)

