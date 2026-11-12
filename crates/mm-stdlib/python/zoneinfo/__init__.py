# zoneinfo: CPython 3.14's Lib/zoneinfo (PSF; THIRD_PARTY.md), the
# pure-Python reader. The sandbox has no time zone files: the host hands the
# engine a zone's TZif bytes (Session::set_zone_source, docs/sdk.md), and
# loaded zones are engine state, so snapshots keep their rules. Without a
# source only ZoneInfo('UTC') loads; other keys raise ZoneInfoNotFoundError.

__all__ = [
    "ZoneInfo",
    "reset_tzpath",
    "available_timezones",
    "TZPATH",
    "ZoneInfoNotFoundError",
    "InvalidTZPathWarning",
]

from zoneinfo import _tzpath
from zoneinfo._common import ZoneInfoNotFoundError
from zoneinfo._zoneinfo import ZoneInfo

reset_tzpath = _tzpath.reset_tzpath
available_timezones = _tzpath.available_timezones
InvalidTZPathWarning = _tzpath.InvalidTZPathWarning


def __getattr__(name):
    if name == "TZPATH":
        return _tzpath.TZPATH
    else:
        raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__():
    return sorted(list(globals()) + ["TZPATH"])
