# zoneinfo._tzpath: CPython 3.14's key validation and reset_tzpath (PSF;
# THIRD_PARTY.md). There is no search path: TZPATH starts empty, and keys
# go to the host's zone source once validated.

import os


def reset_tzpath(to=None):
    global TZPATH
    tzpaths = to
    if tzpaths is not None:
        if isinstance(tzpaths, (str, bytes)):
            raise TypeError(
                f"tzpaths must be a list or tuple, "
                + f"not {type(tzpaths)}: {tzpaths!r}"
            )
        tzpaths = [os.fspath(p) for p in tzpaths]
        if not all(isinstance(p, str) for p in tzpaths):
            raise TypeError(
                "All elements of a tzpath sequence must be strings or "
                "os.PathLike objects which convert to strings."
            )
        if not all(map(os.path.isabs, tzpaths)):
            raise ValueError(_get_invalid_paths_message(tzpaths))
        TZPATH = tuple(tzpaths)
    else:
        TZPATH = ()


def _get_invalid_paths_message(tzpaths):
    invalid_paths = (path for path in tzpaths if not os.path.isabs(path))
    prefix = "\n    "
    indented_str = prefix + prefix.join(invalid_paths)
    return (
        "Paths should be absolute but found the following relative paths:"
        + indented_str
    )


def find_tzfile(key):
    _validate_tzfile_path(key)
    return None


_TEST_PATH = os.path.normpath(os.path.join("_", "_"))[:-1]


def _validate_tzfile_path(path, _base=_TEST_PATH):
    if os.path.isabs(path):
        raise ValueError(
            f"ZoneInfo keys may not be absolute paths, got: {path}"
        )
    new_path = os.path.normpath(path)
    if len(new_path) != len(path):
        raise ValueError(
            f"ZoneInfo keys must be normalized relative paths, got: {path}"
        )
    resolved = os.path.normpath(os.path.join(_base, new_path))
    if not resolved.startswith(_base):
        raise ValueError(
            f"ZoneInfo keys must refer to subdirectories of TZPATH, got: {path}"
        )


del _TEST_PATH


def available_timezones():
    raise NotImplementedError(
        "micro-monty: available_timezones() needs a zone listing, and the "
        "host's zone source only loads zones by key")


class InvalidTZPathWarning(RuntimeWarning):
    """Warning raised if an invalid path is specified in PYTHONTZPATH."""


TZPATH = ()
