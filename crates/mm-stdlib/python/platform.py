"""platform: the interpreter and a fixed sandbox machine (docs/spec.md §12).

Written for micro-monty after CPython's Lib/platform.py. The engine is a
sandbox with no ambient access to its host (docs/spec.md §1, §4), so every
machine query answers with the same constants on every host and target:
system() is 'Linux' (as sys.platform is 'linux'), and node, release,
version, machine and processor are empty, CPython's value when it cannot
tell. The interpreter queries read sys.version_info.
"""

import sys

__all__ = ["system", "node", "release", "version", "machine", "processor",
           "platform", "uname", "uname_result", "architecture",
           "python_implementation", "python_version", "python_version_tuple",
           "python_build", "python_compiler", "python_branch",
           "python_revision", "libc_ver"]


class uname_result(tuple):
    """uname_result(system, node, release, version, machine), with
    processor as a sixth item, as in CPython."""

    _fields = ('system', 'node', 'release', 'version', 'machine', 'processor')

    def __new__(cls, system, node, release, version, machine):
        return tuple.__new__(cls, (system, node, release, version, machine, ''))

    system = property(lambda self: self[0])
    node = property(lambda self: self[1])
    release = property(lambda self: self[2])
    version = property(lambda self: self[3])
    machine = property(lambda self: self[4])
    processor = property(lambda self: self[5])

    def __repr__(self):
        return ('uname_result(system=%r, node=%r, release=%r, version=%r, machine=%r)'
                % tuple(self)[:5])


_UNAME = uname_result('Linux', '', '', '', '')


def uname():
    """The sandbox's fixed (system, node, release, version, machine)."""
    return _UNAME


def system():
    return 'Linux'


def node():
    return ''


def release():
    return ''


def version():
    return ''


def machine():
    return ''


def processor():
    return ''


def platform(aliased=False, terse=False):
    """A single string naming the sandbox platform."""
    return 'Linux'


def architecture(executable='', bits='', linkage=''):
    """('64bit', ''): micro-monty's integers and sys.maxsize are 64-bit."""
    return ('64bit', linkage)


def libc_ver(executable=None, lib='', version='', chunksize=16384):
    return (lib, version)


def python_implementation():
    """'micro-monty', the name sys.version carries."""
    return 'micro-monty'


def python_version():
    return '%d.%d.%d' % sys.version_info[:3]


def python_version_tuple():
    return tuple(['%d' % n for n in sys.version_info[:3]])


def python_build():
    return ('', '')


def python_compiler():
    return ''


def python_branch():
    return ''


def python_revision():
    return ''
