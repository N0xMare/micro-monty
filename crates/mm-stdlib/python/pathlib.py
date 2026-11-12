# pathlib: POSIX pure paths, as CPython 3.14's pathlib parses and prints
# them; the concrete Path's filesystem methods are host calls under Monty's
# names (`Path.read_text`, ...) through os._fs/os._host (docs/spec.md §10).

import os
from os import _fs, _host

__all__ = ['UnsupportedOperation', 'PurePath', 'PurePosixPath', 'Path', 'PosixPath']


class UnsupportedOperation(NotImplementedError):
    pass


def _parse(s):
    # (root, parts) of one POSIX path string.
    if s[:1] == '/':
        root = '//' if s[1:2] == '/' and s[2:3] != '/' else '/'
    else:
        root = ''
    parts = [p for p in s.split('/') if p and p != '.']
    return root, parts


class PurePath(os.PathLike):
    """Base class for manipulating paths without I/O."""

    def __new__(cls, *args):
        if cls is PurePath:
            cls = PurePosixPath
        return object.__new__(cls)

    def __init__(self, *args):
        root = ''
        parts = []
        for a in args:
            if isinstance(a, PurePath):
                r, p = a._root, a._tail
            else:
                if not isinstance(a, str):
                    f = getattr(type(a), '__fspath__', None)
                    a = f(a) if f is not None else a
                if not isinstance(a, str):
                    raise TypeError("argument should be a str or an os.PathLike object where "
                                    "__fspath__ returns a str, not "
                                    f"{type(a).__name__!r}")
                r, p = _parse(a)
            if r:
                root, parts = r, list(p)
            else:
                parts.extend(p)
        self._root = root
        self._tail = parts
        self._str = None

    def with_segments(self, *pathsegments):
        return type(self)(*pathsegments)

    def _from(self, root, tail):
        p = object.__new__(type(self))
        p._root = root
        p._tail = list(tail)
        p._str = None
        return p

    def __str__(self):
        if self._str is None:
            self._str = (self._root + '/'.join(self._tail)) or '.'
        return self._str

    def __fspath__(self):
        return str(self)

    def as_posix(self):
        return str(self)

    def __bytes__(self):
        return os.fsencode(str(self))

    def __repr__(self):
        return f'{type(self).__name__}({self.as_posix()!r})'

    def as_uri(self):
        if not self.is_absolute():
            raise ValueError('relative paths can\'t be expressed as file URIs')
        return 'file://' + str(self)

    def __hash__(self):
        return hash(str(self))

    def __eq__(self, other):
        if not isinstance(other, PurePath):
            return NotImplemented
        return str(self) == str(other)

    def __lt__(self, other):
        if not isinstance(other, PurePath):
            return NotImplemented
        return self.parts < other.parts

    def __le__(self, other):
        if not isinstance(other, PurePath):
            return NotImplemented
        return self.parts <= other.parts

    def __gt__(self, other):
        if not isinstance(other, PurePath):
            return NotImplemented
        return self.parts > other.parts

    def __ge__(self, other):
        if not isinstance(other, PurePath):
            return NotImplemented
        return self.parts >= other.parts

    def __truediv__(self, key):
        if not isinstance(key, (str, PurePath)) and not hasattr(type(key), '__fspath__'):
            return NotImplemented
        try:
            return self.with_segments(self, key)
        except TypeError:
            return NotImplemented

    def __rtruediv__(self, key):
        if not isinstance(key, (str, PurePath)) and not hasattr(type(key), '__fspath__'):
            return NotImplemented
        try:
            return self.with_segments(key, self)
        except TypeError:
            return NotImplemented

    @property
    def drive(self):
        return ''

    @property
    def root(self):
        return self._root

    @property
    def anchor(self):
        return self._root

    @property
    def parts(self):
        if self._root:
            return (self._root,) + tuple(self._tail)
        return tuple(self._tail)

    @property
    def name(self):
        return self._tail[-1] if self._tail else ''

    @property
    def suffix(self):
        name = self.name
        i = name.rfind('.')
        if 0 < i < len(name) - 1:
            return name[i:]
        return ''

    @property
    def suffixes(self):
        name = self.name
        if name.endswith('.'):
            return []
        name = name.lstrip('.')
        return ['.' + s for s in name.split('.')[1:]]

    @property
    def stem(self):
        name = self.name
        i = name.rfind('.')
        if 0 < i < len(name) - 1:
            return name[:i]
        return name

    @property
    def parent(self):
        if not self._tail:
            return self
        return self._from(self._root, self._tail[:-1])

    @property
    def parents(self):
        out = []
        p = self
        while p._tail:
            p = p.parent
            out.append(p)
        return tuple(out)

    def is_absolute(self):
        return bool(self._root)

    def is_reserved(self):
        return False

    def joinpath(self, *pathsegments):
        return self.with_segments(self, *pathsegments)

    def with_name(self, name):
        if not self.name:
            raise ValueError(f'{self!r} has an empty name')
        if not name or '/' in name or name == '.':
            raise ValueError(f'Invalid name {name!r}')
        return self._from(self._root, self._tail[:-1] + [name])

    def with_stem(self, stem):
        suffix = self.suffix
        if not suffix:
            return self.with_name(stem)
        if not stem:
            raise ValueError(f'{self!r} has a non-empty suffix')
        return self.with_name(stem + suffix)

    def with_suffix(self, suffix):
        stem = self.stem
        if not stem:
            raise ValueError(f'{self!r} has an empty name')
        if suffix and (not suffix.startswith('.') or suffix == '.' or '/' in suffix):
            raise ValueError(f'Invalid suffix {suffix!r}')
        return self.with_name(stem + suffix)

    def relative_to(self, other, *, walk_up=False):
        if not isinstance(other, PurePath):
            other = self.with_segments(other)
        if self._root != other._root:
            raise ValueError(f'{str(self)!r} is not in the subpath of {str(other)!r}')
        n = len(other._tail)
        if self._tail[:n] == other._tail:
            return self._from('', self._tail[n:])
        if not walk_up:
            raise ValueError(f'{str(self)!r} is not in the subpath of {str(other)!r}')
        common = 0
        while (common < n and common < len(self._tail)
               and self._tail[common] == other._tail[common]):
            common += 1
        if '..' in other._tail[common:]:
            raise ValueError(f"'..' segment in {str(other)!r} cannot be walked")
        return self._from('', ['..'] * (n - common) + self._tail[common:])

    def is_relative_to(self, other):
        if not isinstance(other, PurePath):
            other = self.with_segments(other)
        return self._root == other._root and self._tail[:len(other._tail)] == other._tail

    def full_match(self, pattern, *, case_sensitive=None):
        from fnmatch import fnmatchcase
        return fnmatchcase(str(self), str(pattern))

    def match(self, path_pattern, *, case_sensitive=None):
        from fnmatch import fnmatchcase
        pat = self.with_segments(path_pattern)
        if pat._root:
            if pat._root != self._root or len(pat._tail) != len(self._tail):
                return False
        elif len(pat._tail) > len(self._tail):
            return False
        if not pat._tail:
            raise ValueError('empty pattern')
        for part, pp in zip(reversed(self._tail), reversed(pat._tail)):
            if not fnmatchcase(part, pp):
                return False
        return True


class PurePosixPath(PurePath):
    """PurePath subclass for non-Windows systems."""


class Path(PurePath):
    """PurePath subclass that can make system calls (here: host calls)."""

    def __new__(cls, *args, **kwargs):
        if cls is Path:
            cls = PosixPath
        return object.__new__(cls)

    def _path(self):
        return str(self)

    def stat(self, *, follow_symlinks=True):
        return os._stat_result(_fs('Path.stat', self._path()))

    def lstat(self):
        return self.stat(follow_symlinks=False)

    def exists(self, *, follow_symlinks=True):
        return _fs('Path.exists', self._path())

    def is_file(self, *, follow_symlinks=True):
        return _fs('Path.is_file', self._path())

    def is_dir(self, *, follow_symlinks=True):
        return _fs('Path.is_dir', self._path())

    def is_symlink(self):
        return _fs('Path.is_symlink', self._path())

    def is_mount(self):
        return os.path.ismount(self._path())

    def is_junction(self):
        return False

    def read_text(self, encoding=None, errors=None, newline=None):
        return _fs('Path.read_text', self._path())

    def read_bytes(self):
        return _fs('Path.read_bytes', self._path())

    def write_text(self, data, encoding=None, errors=None, newline=None):
        if not isinstance(data, str):
            raise TypeError(f'data must be str, not {type(data).__name__}')
        return _fs('Path.write_text', self._path(), data)

    def write_bytes(self, data):
        if not isinstance(data, bytes):
            raise TypeError("memoryview: a bytes-like object is required, not "
                            f"'{type(data).__name__}'")
        return _fs('Path.write_bytes', self._path(), data)

    def open(self, mode='r', buffering=-1, encoding=None, errors=None, newline=None):
        return open(self._path(), mode, buffering, encoding, errors, newline)

    def iterdir(self):
        return iter([self.with_segments(os.fsdecode(e)) for e in
                     _fs('Path.iterdir', self._path())])

    # Scans are one host call, `Path.scan(path, kind, pattern)`: the host
    # matches `glob`/`rglob` patterns and answers path strings, or answers
    # `walk`'s (dirpath, dirnames, filenames) triples.
    def glob(self, pattern, *, case_sensitive=None, recurse_symlinks=False):
        return iter([self.with_segments(os.fsdecode(e)) for e in
                     _host('Path.scan', self._path(), 'glob', str(pattern))])

    def rglob(self, pattern, *, case_sensitive=None, recurse_symlinks=False):
        return iter([self.with_segments(os.fsdecode(e)) for e in
                     _host('Path.scan', self._path(), 'rglob', str(pattern))])

    def walk(self, top_down=True, on_error=None, follow_symlinks=False):
        return iter([(self.with_segments(d), list(ds), list(fs)) for d, ds, fs in
                     _host('Path.scan', self._path(), 'walk', None)])

    def mkdir(self, mode=0o777, parents=False, exist_ok=False):
        _fs('Path.mkdir', self._path(), bool(parents), bool(exist_ok))

    def touch(self, mode=0o666, exist_ok=True):
        if not _fs('Path.exists', self._path()):
            _fs('Path.write_bytes', self._path(), b'')
        elif not exist_ok:
            raise FileExistsError(17, 'File exists', self._path())

    def unlink(self, missing_ok=False):
        if missing_ok and not _fs('Path.exists', self._path()):
            return
        _fs('Path.unlink', self._path())

    def rmdir(self):
        _fs('Path.rmdir', self._path())

    def rename(self, target):
        target = self.with_segments(target)
        _fs('Path.rename', self._path(), str(target))
        return target

    def replace(self, target):
        return self.rename(target)

    def resolve(self, strict=False):
        return self.with_segments(_fs('Path.resolve', self._path()))

    def absolute(self):
        if self.is_absolute():
            return self
        return self.with_segments(os.getcwd(), self)

    def expanduser(self):
        return self.with_segments(os.path.expanduser(str(self)))

    def samefile(self, other_path):
        return os.path.samefile(self._path(), os.fspath(other_path))

    @classmethod
    def cwd(cls):
        return cls(os.getcwd())

    @classmethod
    def home(cls):
        return cls(os.path.expanduser('~'))


class PosixPath(Path, PurePosixPath):
    """Path subclass for non-Windows systems."""

