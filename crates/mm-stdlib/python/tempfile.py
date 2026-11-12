# tempfile: the sandbox has no ambient filesystem (docs/spec.md §1, §10), so
# the module exists for code that imports it, and every function or class
# that would create or look for a file or directory raises PermissionError,
# as open() does. There is no host hook for these: a host with storage
# offers its own functions. gettempprefix() and the constants are pure.

__all__ = ["NamedTemporaryFile", "TemporaryFile", "SpooledTemporaryFile",
           "TemporaryDirectory", "mkstemp", "mkdtemp", "mktemp", "TMP_MAX",
           "gettempprefix", "tempdir", "gettempdir", "gettempprefixb",
           "gettempdirb"]

TMP_MAX = 20
template = "tmp"
tempdir = None


def _unavailable(name):
    import _mm
    hint = _mm.hint("os")
    raise PermissionError(f"tempfile.{name}() is not available: this sandbox has no "
                          "ambient filesystem" + (". " + hint if hint else ""))


def gettempprefix():
    """The default prefix for temporary directories as a string."""
    return template


def gettempprefixb():
    """The default prefix for temporary directories as bytes."""
    return template.encode()


def gettempdir():
    _unavailable("gettempdir")


def gettempdirb():
    _unavailable("gettempdirb")


def mkstemp(suffix=None, prefix=None, dir=None, text=False):
    _unavailable("mkstemp")


def mkdtemp(suffix=None, prefix=None, dir=None):
    _unavailable("mkdtemp")


def mktemp(suffix="", prefix=template, dir=None):
    _unavailable("mktemp")


def NamedTemporaryFile(*args, **kwargs):
    _unavailable("NamedTemporaryFile")


def TemporaryFile(*args, **kwargs):
    _unavailable("TemporaryFile")


class SpooledTemporaryFile:
    """Temporary file wrapper: not available in the sandbox."""

    def __init__(self, *args, **kwargs):
        _unavailable("SpooledTemporaryFile")


class TemporaryDirectory:
    """Temporary directory context manager: not available in the sandbox."""

    def __init__(self, *args, **kwargs):
        _unavailable("TemporaryDirectory")
