# os.path: CPython 3.14's Lib/posixpath.py and the pure half of
# Lib/genericpath.py; the filesystem queries are host calls (docs/spec.md
# §10). PSF license; see THIRD_PARTY.md.

# Strings representing various path-related bits and pieces.
# These are primarily for export; internally, they are hardcoded.
# Should be set before imports for resolving cyclic dependency.
curdir = '.'
pardir = '..'
extsep = '.'
sep = '/'
pathsep = ':'
defpath = '/bin:/usr/bin'
altsep = None
devnull = '/dev/null'

import os

__all__ = ["normcase","isabs","join","splitdrive","splitroot","split","splitext",
           "basename","dirname","commonprefix","getsize","getmtime",
           "getatime","getctime","islink","exists","lexists","isdir","isfile",
           "ismount", "expanduser","expandvars","normpath","abspath",
           "samefile","sameopenfile","samestat",
           "curdir","pardir","sep","pathsep","defpath","altsep","extsep",
           "devnull","realpath","supports_unicode_filenames","relpath",
           "commonpath", "isjunction","isdevdrive","ALLOW_MISSING"]


# The filesystem queries ask the host (docs/spec.md §10): os._fs names the
# host function (Monty's names) and refuses when the host registered none.

def _query(op, fname, path, what):
    # A filesystem predicate: posixmodule's argument check, then the host
    # (the empty path and file descriptors name no file here).
    path = os._path_arg(fname, path, what)
    if isinstance(path, int) or not path:
        return False
    return os._fs(op, os.fsdecode(path))


_STAT = 'string, bytes, os.PathLike or integer'
_LSTAT = 'string, bytes or os.PathLike'


def exists(path):
    """Test whether a path exists."""
    return _query('Path.exists', 'stat', path, _STAT)


def lexists(path):
    """Test whether a path exists.  Returns True for broken symbolic links"""
    return _query('Path.exists', 'lstat', path, _LSTAT)


def isfile(path):
    """Test whether a path is a regular file"""
    return _query('Path.is_file', 'stat', path, _STAT)


def isdir(s):
    """Return true if the pathname refers to an existing directory."""
    return _query('Path.is_dir', 'stat', s, _STAT)


def islink(path):
    """Test whether a path is a symbolic link"""
    return _query('Path.is_symlink', 'lstat', path, _LSTAT)


def isjunction(path):
    os.fspath(path)
    return False


def isdevdrive(path):
    os.fspath(path)
    return False


def getsize(filename):
    """Return the size of a file, reported by os.stat()."""
    return os.stat(filename).st_size


def getmtime(filename):
    """Return the last modification time of a file, reported by os.stat()."""
    return os.stat(filename).st_mtime


def getatime(filename):
    """Return the last access time of a file, reported by os.stat()."""
    return os.stat(filename).st_atime


def getctime(filename):
    """Return the metadata change time of a file, reported by os.stat()."""
    return os.stat(filename).st_ctime


def commonprefix(m):
    "Given a list of pathnames, returns the longest common leading component"
    if not m: return ''
    if not isinstance(m[0], (list, tuple)):
        m = tuple(map(os.fspath, m))
    s1 = min(m)
    s2 = max(m)
    for i, c in enumerate(s1):
        if c != s2[i]:
            return s1[:i]
    return s1


def samestat(s1, s2):
    """Test whether two stat buffers reference the same file"""
    return (s1.st_ino == s2.st_ino and
            s1.st_dev == s2.st_dev)


def samefile(f1, f2):
    s1 = os.stat(f1)
    s2 = os.stat(f2)
    return samestat(s1, s2)


def sameopenfile(fp1, fp2):
    s1 = os.fstat(fp1)
    s2 = os.fstat(fp2)
    return samestat(s1, s2)


def _splitext(p, sep, altsep, extsep):
    sepIndex = p.rfind(sep)
    if altsep:
        altsepIndex = p.rfind(altsep)
        sepIndex = max(sepIndex, altsepIndex)
    dotIndex = p.rfind(extsep)
    if dotIndex > sepIndex:
        filenameIndex = sepIndex + 1
        while filenameIndex < dotIndex:
            if p[filenameIndex:filenameIndex+1] != extsep:
                return p[:dotIndex], p[dotIndex:]
            filenameIndex += 1
    return p, p[:0]


def _check_arg_types(funcname, *args):
    hasstr = hasbytes = False
    for s in args:
        if isinstance(s, str):
            hasstr = True
        elif isinstance(s, bytes):
            hasbytes = True
        else:
            raise TypeError(f'{funcname}() argument must be str, bytes, or '
                            f'os.PathLike object, not {s.__class__.__name__!r}') from None
    if hasstr and hasbytes:
        raise TypeError("Can't mix strings and bytes in path components") from None


class _AllowMissing:
    def __repr__(self):
        return 'os.path.ALLOW_MISSING'


ALLOW_MISSING = _AllowMissing()


def _get_sep(path):
    if isinstance(path, bytes):
        return b'/'
    else:
        return '/'

# Normalize the case of a pathname.  Trivial in Posix, string.lower on Mac.
# On MS-DOS this may also turn slashes into backslashes; however, other
# normalizations (such as optimizing '../' away) are not allowed
# (another function should be defined to do that).

def normcase(s):
    """Normalize case of pathname.  Has no effect under Posix"""
    return os.fspath(s)


# Return whether a path is absolute.
# Trivial in Posix, harder on the Mac or MS-DOS.

def isabs(s):
    """Test whether a path is absolute"""
    s = os.fspath(s)
    sep = _get_sep(s)
    return s.startswith(sep)


# Join pathnames.
# Ignore the previous parts if a part is absolute.
# Insert a '/' unless the first part is empty or already ends in '/'.

def join(a, *p):
    """Join two or more pathname components, inserting '/' as needed.
    If any component is an absolute path, all previous path components
    will be discarded.  An empty last part will result in a path that
    ends with a separator."""
    a = os.fspath(a)
    sep = _get_sep(a)
    path = a
    try:
        for b in p:
            b = os.fspath(b)
            if b.startswith(sep) or not path:
                path = b
            elif path.endswith(sep):
                path += b
            else:
                path += sep + b
    except (TypeError, AttributeError):
        _check_arg_types('join', a, *p)
        raise
    return path


# Split a path in head (everything up to the last '/') and tail (the
# rest).  If the path ends in '/', tail will be empty.  If there is no
# '/' in the path, head  will be empty.
# Trailing '/'es are stripped from head unless it is the root.

def split(p):
    """Split a pathname.  Returns tuple "(head, tail)" where "tail" is
    everything after the final slash.  Either part may be empty."""
    p = os.fspath(p)
    sep = _get_sep(p)
    i = p.rfind(sep) + 1
    head, tail = p[:i], p[i:]
    if head and head != sep*len(head):
        head = head.rstrip(sep)
    return head, tail


# Split a path in root and extension.
# The extension is everything starting at the last dot in the last
# pathname component; the root is everything before that.
# It is always true that root + ext == p.

def splitext(p):
    p = os.fspath(p)
    if isinstance(p, bytes):
        sep = b'/'
        extsep = b'.'
    else:
        sep = '/'
        extsep = '.'
    return _splitext(p, sep, None, extsep)
splitext.__doc__ = _splitext.__doc__

# Split a pathname into a drive specification and the rest of the
# path.  Useful on DOS/Windows/NT; on Unix, the drive is always empty.

def splitdrive(p):
    """Split a pathname into drive and path. On Posix, drive is always
    empty."""
    p = os.fspath(p)
    return p[:0], p


def splitroot(*args, **kwargs):
    """Split a pathname into drive, root and tail.

    The tail contains anything after the root."""
    # posixmodule.c's _path_splitroot_ex, with its argument errors.
    p, = os._clinic('_path_splitroot_ex', args, kwargs, ('p',), 1, 1)
    p = os._path_arg('_path_splitroot_ex', p, 'string, bytes or os.PathLike')
    if isinstance(p, bytes):
        sep = b'/'
        empty = b''
    else:
        sep = '/'
        empty = ''
    if p[:1] != sep:
        # Relative path, e.g.: 'foo'
        return empty, empty, p
    elif p[1:2] != sep or p[2:3] == sep:
        # Absolute path, e.g.: '/foo', '///foo', '////foo', etc.
        return empty, sep, p[1:]
    else:
        # Precisely two leading slashes, e.g.: '//foo'. Implementation defined per POSIX, see
        # https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap04.html#tag_04_13
        return empty, p[:2], p[2:]


# Return the tail (basename) part of a path, same as split(path)[1].

def basename(p):
    """Returns the final component of a pathname"""
    p = os.fspath(p)
    sep = _get_sep(p)
    i = p.rfind(sep) + 1
    return p[i:]


# Return the head (dirname) part of a path, same as split(path)[0].

def dirname(p):
    """Returns the directory component of a pathname"""
    p = os.fspath(p)
    sep = _get_sep(p)
    i = p.rfind(sep) + 1
    head = p[:i]
    if head and head != sep*len(head):
        head = head.rstrip(sep)
    return head


# Is a path a mount point?
# (Does this work for all UNIXes?  Is it even guaranteed to work by Posix?)

def ismount(path):
    """Test whether a path is a mount point"""
    return _query('os.path.ismount', 'lstat', path, _LSTAT)


# Expand paths beginning with '~' or '~user'.
# '~' means $HOME; '~user' means that user's home directory.
# If the path doesn't begin with '~', or if the user or $HOME is unknown,
# the path is returned unchanged (leaving error reporting to whatever
# function is called with the expanded path as argument).
# See also module 'glob' for expansion of *, ? and [...] in pathnames.
# (A function should also be defined to do full *sh-style environment
# variable expansion.)

def expanduser(path):
    """Expand ~ and ~user constructions.  If user or $HOME is unknown,
    do nothing."""
    path = os.fspath(path)
    if isinstance(path, bytes):
        tilde = b'~'
    else:
        tilde = '~'
    if not path.startswith(tilde):
        return path
    sep = _get_sep(path)
    i = path.find(sep, 1)
    if i < 0:
        i = len(path)
    if i == 1:
        if 'HOME' not in os.environ:
            try:
                import pwd
            except ImportError:
                # pwd module unavailable, return path unchanged
                return path
            try:
                userhome = pwd.getpwuid(os.getuid()).pw_dir
            except KeyError:
                # bpo-10496: if the current user identifier doesn't exist in the
                # password database, return the path unchanged
                return path
        else:
            userhome = os.environ['HOME']
    else:
        try:
            import pwd
        except ImportError:
            # pwd module unavailable, return path unchanged
            return path
        name = path[1:i]
        if isinstance(name, bytes):
            name = os.fsdecode(name)
        try:
            pwent = pwd.getpwnam(name)
        except KeyError:
            # bpo-10496: if the user name from the path doesn't exist in the
            # password database, return the path unchanged
            return path
        userhome = pwent.pw_dir
    # if no user home, return the path unchanged on VxWorks
    if userhome is None and False:
        return path
    if isinstance(path, bytes):
        userhome = os.fsencode(userhome)
    userhome = userhome.rstrip(sep)
    return (userhome + path[i:]) or sep


# Expand paths containing shell variable substitutions.
# This expands the forms $variable and ${variable} only.
# Non-existent variables are left unchanged.

_varpattern = r'\$(\w+|\{[^}]*\}?)'
_varsub = None
_varsubb = None

def expandvars(path):
    """Expand shell variables of form $var and ${var}.  Unknown variables
    are left unchanged."""
    path = os.fspath(path)
    global _varsub, _varsubb
    if isinstance(path, bytes):
        if b'$' not in path:
            return path
        if not _varsubb:
            import re
            _varsubb = re.compile(_varpattern.encode(), re.ASCII).sub
        sub = _varsubb
        start = b'{'
        end = b'}'
        environ = getattr(os, 'environb', None)
    else:
        if '$' not in path:
            return path
        if not _varsub:
            import re
            _varsub = re.compile(_varpattern, re.ASCII).sub
        sub = _varsub
        start = '{'
        end = '}'
        environ = os.environ

    def repl(m):
        name = m[1]
        if name.startswith(start):
            if not name.endswith(end):
                return m[0]
            name = name[1:-1]
        try:
            if environ is None:
                value = os.fsencode(os.environ[os.fsdecode(name)])
            else:
                value = environ[name]
        except KeyError:
            return m[0]
        else:
            return value

    return sub(repl, path)


# Normalize a path, e.g. A//B, A/./B and A/foo/../B all become A/B.
# It should be understood that this may change the meaning of the path
# if it contains symbolic links!

def normpath(*args, **kwargs):
    """Normalize path, eliminating double slashes, etc."""
    # posixmodule.c's _path_normpath, with its argument errors.
    path, = os._clinic('_path_normpath', args, kwargs, ('path',), 1, 1)
    path = os._path_arg('_path_normpath', path, 'string, bytes or os.PathLike')
    if isinstance(path, bytes):
        sep = b'/'
        dot = b'.'
        dotdot = b'..'
    else:
        sep = '/'
        dot = '.'
        dotdot = '..'
    if not path:
        return dot
    _, initial_slashes, path = splitroot(path)
    comps = path.split(sep)
    new_comps = []
    for comp in comps:
        if not comp or comp == dot:
            continue
        if (comp != dotdot or (not initial_slashes and not new_comps) or
             (new_comps and new_comps[-1] == dotdot)):
            new_comps.append(comp)
        elif new_comps:
            new_comps.pop()
    comps = new_comps
    path = initial_slashes + sep.join(comps)
    return path or dot


def abspath(path):
    """Return an absolute path."""
    path = os.fspath(path)
    if isinstance(path, bytes):
        if not path.startswith(b'/'):
            path = join(os.getcwdb(), path)
    else:
        if not path.startswith('/'):
            path = join(os.getcwd(), path)
    return normpath(path)


# Return a canonical path (i.e. the absolute location of a file on the
# filesystem).

def realpath(filename, *, strict=False):
    """Return the canonical path of the specified filename, eliminating any
symbolic links encountered in the path."""
    filename = os.fspath(filename)
    if not filename:
        return os.getcwdb() if isinstance(filename, bytes) else os.getcwd()
    r = os._fs('Path.resolve', os.fsdecode(abspath(filename)))
    return os.fsencode(r) if isinstance(filename, bytes) else r


supports_unicode_filenames = False

def relpath(path, start=None):
    """Return a relative version of a path"""

    path = os.fspath(path)
    if not path:
        raise ValueError("no path specified")

    if isinstance(path, bytes):
        curdir = b'.'
        sep = b'/'
        pardir = b'..'
    else:
        curdir = '.'
        sep = '/'
        pardir = '..'

    if start is None:
        start = curdir
    else:
        start = os.fspath(start)

    try:
        start_tail = abspath(start).lstrip(sep)
        path_tail = abspath(path).lstrip(sep)
        start_list = start_tail.split(sep) if start_tail else []
        path_list = path_tail.split(sep) if path_tail else []
        # Work out how much of the filepath is shared by start and path.
        i = len(commonprefix([start_list, path_list]))

        rel_list = [pardir] * (len(start_list)-i) + path_list[i:]
        if not rel_list:
            return curdir
        return sep.join(rel_list)
    except (TypeError, AttributeError):
        _check_arg_types('relpath', path, start)
        raise


# Return the longest common sub-path of the sequence of paths given as input.
# The paths are not normalized before comparing them (this is the
# responsibility of the caller). Any trailing separator is stripped from the
# returned path.

def commonpath(paths):
    """Given a sequence of path names, returns the longest common sub-path."""

    paths = tuple(map(os.fspath, paths))

    if not paths:
        raise ValueError('commonpath() arg is an empty sequence')

    if isinstance(paths[0], bytes):
        sep = b'/'
        curdir = b'.'
    else:
        sep = '/'
        curdir = '.'

    try:
        split_paths = [path.split(sep) for path in paths]

        try:
            isabs, = {p.startswith(sep) for p in paths}
        except ValueError:
            raise ValueError("Can't mix absolute and relative paths") from None

        split_paths = [[c for c in s if c and c != curdir] for s in split_paths]
        s1 = min(split_paths)
        s2 = max(split_paths)
        common = s1
        for i, c in enumerate(s1):
            if c != s2[i]:
                common = s1[:i]
                break

        prefix = sep if isabs else sep[:0]
        return prefix + sep.join(common)
    except (TypeError, AttributeError):
        _check_arg_types('commonpath', *paths)
        raise
