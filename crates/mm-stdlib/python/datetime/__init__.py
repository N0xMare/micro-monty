# datetime: CPython 3.14's Lib/_pydatetime.py, adapted (no `%` formatting,
# no __slots__; strptime from datetime._strptime; the local zone is the time
# module's). PSF license; see THIRD_PARTY.md and docs/spec.md §12.

__all__ = ("date", "datetime", "time", "timedelta", "timezone", "tzinfo",
           "MINYEAR", "MAXYEAR", "UTC")



import time as _time
import math as _math
from operator import index as _index
from _mm import dt_iso as _dt_iso, dt_ord as _dt_ord, dt_ymd as _dt_ymd

_MISSING = object()


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


def _str_arg(fname, pos, v):
    if not isinstance(v, str):
        raise TypeError(f"{fname}() argument {pos} must be str, not "
                        f"{'None' if v is None else type(v).__name__}")
    return v


def _getargs(args, kwargs, names, maxpos, minreq):
    # PyArg_ParseTupleAndKeywords for a format without a function name.
    nargs = len(args)
    n = len(names)
    if nargs + len(kwargs) > n:
        raise TypeError(f"function takes at most {n} {'keyword ' if nargs == 0 else ''}"
                        f"argument{'' if n == 1 else 's'} ({nargs + len(kwargs)} given)")
    if nargs > maxpos:
        raise TypeError(f"function takes {'at most' if minreq < maxpos else 'exactly'} {maxpos} "
                        f"positional argument{'' if maxpos == 1 else 's'} ({nargs} given)")
    out = list(args) + [_MISSING] * (n - nargs)
    for i in range(nargs, n):
        if names[i] in kwargs:
            out[i] = kwargs[names[i]]
        elif i < minreq:
            raise TypeError(f"function missing required argument '{names[i]}' (pos {i + 1})")
    for k in kwargs:
        if k in names:
            i = names.index(k)
            if i < nargs:
                raise TypeError(f"argument for function given by name ('{k}') and position "
                                f"({i + 1})")
        else:
            raise TypeError(f"this function got an unexpected keyword argument '{k}'")
    return out


def _cint(x):
    # The "i" converter: __index__, then the C int range.
    x = _index(x)
    if not -0x8000000000000000 <= x <= 0x7FFFFFFFFFFFFFFF:
        raise OverflowError('Python int too large to convert to C long')
    if x > 0x7FFFFFFF:
        raise OverflowError('signed integer is greater than maximum')
    if x < -0x80000000:
        raise OverflowError('signed integer is less than minimum')
    return x


def _noargs(name, args, kwargs):
    # METH_NOARGS's messages.
    if kwargs:
        raise TypeError(f'{name}() takes no keyword arguments')
    if args:
        raise TypeError(f'{name}() takes no arguments ({len(args)} given)')


def _meth_o(name, args, kwargs):
    # METH_O's messages.
    if kwargs:
        raise TypeError(f'{name}() takes no keyword arguments')
    if len(args) != 1:
        raise TypeError(f'{name}() takes exactly one argument ({len(args)} given)')
    return args[0]


def _cmp(x, y):
    return 0 if x == y else 1 if x > y else -1

def _get_class_module(self):
    module_name = self.__class__.__module__
    if module_name == 'datetime':
        return 'datetime.'
    else:
        return ''

MINYEAR = 1
MAXYEAR = 9999
_MAXORDINAL = 3652059  # date.max.toordinal()

# Utility functions, adapted from Python's Demo/classes/Dates.py, which
# also assumes the current Gregorian calendar indefinitely extended in
# both directions.  Difference:  Dates.py calls January 1 of year 0 day
# number 1.  The code here calls January 1 of year 1 day number 1.  This is
# to match the definition of the "proleptic Gregorian" calendar in Dershowitz
# and Reingold's "Calendrical Calculations", where it's the base calendar
# for all computations.  See the book for algorithms for converting between
# proleptic Gregorian ordinals and many other calendar systems.

# -1 is a placeholder for indexing purposes.
_DAYS_IN_MONTH = [-1, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]

_DAYS_BEFORE_MONTH = [-1]  # -1 is a placeholder for indexing purposes.
dbm = 0
for dim in _DAYS_IN_MONTH[1:]:
    _DAYS_BEFORE_MONTH.append(dbm)
    dbm += dim
del dbm, dim

def _is_leap(year):
    "year -> 1 if leap year, else 0."
    return year % 4 == 0 and (year % 100 != 0 or year % 400 == 0)

def _days_before_year(year):
    "year -> number of days before January 1st of year."
    y = year - 1
    return y*365 + y//4 - y//100 + y//400

def _days_in_month(year, month):
    "year, month -> number of days in that month in that year."
    assert 1 <= month <= 12, f"month must be in 1..12, not {month}"
    if month == 2 and _is_leap(year):
        return 29
    return _DAYS_IN_MONTH[month]

def _days_before_month(year, month):
    "year, month -> number of days in year preceding first day of month."
    assert 1 <= month <= 12, f"month must be in 1..12, not {month}"
    return _DAYS_BEFORE_MONTH[month] + (month > 2 and _is_leap(year))

def _ymd2ord(year, month, day):
    "year, month, day -> ordinal, considering 01-Jan-0001 as day 1."
    n = _dt_ord(year, month, day)
    if n is not None:
        return n
    assert 1 <= month <= 12, f"month must be in 1..12, not {month}"
    dim = _days_in_month(year, month)
    assert 1 <= day <= dim, f"day must be in 1..{dim}, not {day}"
    return (_days_before_year(year) +
            _days_before_month(year, month) +
            day)

_DI400Y = _days_before_year(401)    # number of days in 400 years
_DI100Y = _days_before_year(101)    #    "    "   "   " 100   "
_DI4Y   = _days_before_year(5)      #    "    "   "   "   4   "

# A 4-year cycle has an extra leap day over what we'd get from pasting
# together 4 single years.
assert _DI4Y == 4 * 365 + 1

# Similarly, a 400-year cycle has an extra leap day over what we'd get from
# pasting together 4 100-year cycles.
assert _DI400Y == 4 * _DI100Y + 1

# OTOH, a 100-year cycle has one fewer leap day than we'd get from
# pasting together 25 4-year cycles.
assert _DI100Y == 25 * _DI4Y - 1

def _ord2ymd(n):
    "ordinal -> (year, month, day), considering 01-Jan-0001 as day 1."
    r = _dt_ymd(n)
    if r is not None:
        return r

    # n is a 1-based index, starting at 1-Jan-1.  The pattern of leap years
    # repeats exactly every 400 years.  The basic strategy is to find the
    # closest 400-year boundary at or before n, then work with the offset
    # from that boundary to n.  Life is much clearer if we subtract 1 from
    # n first -- then the values of n at 400-year boundaries are exactly
    # those divisible by _DI400Y:
    #
    #     D  M   Y            n              n-1
    #     -- --- ----        ----------     ----------------
    #     31 Dec -400        -_DI400Y       -_DI400Y -1
    #      1 Jan -399         -_DI400Y +1   -_DI400Y      400-year boundary
    #     ...
    #     30 Dec  000        -1             -2
    #     31 Dec  000         0             -1
    #      1 Jan  001         1              0            400-year boundary
    #      2 Jan  001         2              1
    #      3 Jan  001         3              2
    #     ...
    #     31 Dec  400         _DI400Y        _DI400Y -1
    #      1 Jan  401         _DI400Y +1     _DI400Y      400-year boundary
    n -= 1
    n400, n = divmod(n, _DI400Y)
    year = n400 * 400 + 1   # ..., -399, 1, 401, ...

    # Now n is the (non-negative) offset, in days, from January 1 of year, to
    # the desired date.  Now compute how many 100-year cycles precede n.
    # Note that it's possible for n100 to equal 4!  In that case 4 full
    # 100-year cycles precede the desired day, which implies the desired
    # day is December 31 at the end of a 400-year cycle.
    n100, n = divmod(n, _DI100Y)

    # Now compute how many 4-year cycles precede it.
    n4, n = divmod(n, _DI4Y)

    # And now how many single years.  Again n1 can be 4, and again meaning
    # that the desired day is December 31 at the end of the 4-year cycle.
    n1, n = divmod(n, 365)

    year += n100 * 100 + n4 * 4 + n1
    if n1 == 4 or n100 == 4:
        assert n == 0
        return year-1, 12, 31

    # Now the year is correct, and n is the offset from January 1.  We find
    # the month via an estimate that's either exact or one too large.
    leapyear = n1 == 3 and (n4 != 24 or n100 == 3)
    assert leapyear == _is_leap(year)
    month = (n + 50) >> 5
    preceding = _DAYS_BEFORE_MONTH[month] + (month > 2 and leapyear)
    if preceding > n:  # estimate is too large
        month -= 1
        preceding -= _DAYS_IN_MONTH[month] + (month == 2 and leapyear)
    n -= preceding
    assert 0 <= n < _days_in_month(year, month)

    # Now the year and month are correct, and n is the offset from the
    # start of that month:  we're done!
    return year, month, n+1

# Month and day names.  For localized versions, see the calendar module.
_MONTHNAMES = [None, "Jan", "Feb", "Mar", "Apr", "May", "Jun",
                     "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
_DAYNAMES = [None, "Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]


def _build_struct_time(y, m, d, hh, mm, ss, dstflag):
    wday = (_ymd2ord(y, m, d) + 6) % 7
    dnum = _days_before_month(y, m) + d
    return _time.struct_time((y, m, d, hh, mm, ss, wday, dnum, dstflag))

_TIME_SPECS = {
    'hours': '{:02d}',
    'minutes': '{:02d}:{:02d}',
    'seconds': '{:02d}:{:02d}:{:02d}',
    'milliseconds': '{:02d}:{:02d}:{:02d}.{:03d}',
    'microseconds': '{:02d}:{:02d}:{:02d}.{:06d}'
}

def _format_time(hh, mm, ss, us, timespec='auto'):
    specs = _TIME_SPECS
    if timespec == 'auto':
        # Skip trailing microseconds when us==0.
        timespec = 'microseconds' if us else 'seconds'
    elif timespec == 'milliseconds':
        us //= 1000
    try:
        fmt = specs[timespec]
    except KeyError:
        raise ValueError('Unknown timespec value')
    else:
        return fmt.format(hh, mm, ss, us)

def _format_offset(off, sep=':'):
    # format_utcoffset, on the offset's fields.
    if off is None:
        return ''
    us = (off._days * 86400 + off._seconds) * 1000000 + off._microseconds
    if us < 0:
        sign = "-"
        us = -us
    else:
        sign = "+"
    ss, us = divmod(us, 1000000)
    mm, ss = divmod(ss, 60)
    hh, mm = divmod(mm, 60)
    s = f"{sign}{hh:02d}{sep}{mm:02d}"
    if ss or us:
        s += f"{sep}{ss:02d}"
        if us:
            s += f'.{us:06d}'
    return s

_normalize_century = None
def _need_normalize_century():
    global _normalize_century
    if _normalize_century is None:
        try:
            _normalize_century = (
                _time.strftime("%Y", (99, 1, 1, 0, 0, 0, 0, 1, 0)) != "0099")
        except ValueError:
            _normalize_century = True
    return _normalize_century

# Correctly substitute for %z and %Z escapes in strftime formats.
def _wrap_strftime(object, format, timetuple):
    # Don't call utcoffset() or tzname() unless actually needed.
    freplace = None  # the string to use for %f
    zreplace = None  # the string to use for %z
    colonzreplace = None  # the string to use for %:z
    Zreplace = None  # the string to use for %Z

    # Scan format for %z, %:z and %Z escapes, replacing as needed.
    newformat = []
    push = newformat.append
    i, n = 0, len(format)
    while i < n:
        ch = format[i]
        i += 1
        if ch == '%':
            if i < n:
                ch = format[i]
                i += 1
                if ch == 'f':
                    if freplace is None:
                        freplace = f"{getattr(object, 'microsecond', 0):06d}"
                    newformat.append(freplace)
                elif ch == 'z':
                    if zreplace is None:
                        if hasattr(object, "utcoffset"):
                            zreplace = _format_offset(object.utcoffset(), sep="")
                        else:
                            zreplace = ""
                    assert '%' not in zreplace
                    newformat.append(zreplace)
                elif ch == ':':
                    if i < n:
                        ch2 = format[i]
                        i += 1
                        if ch2 == 'z':
                            if colonzreplace is None:
                                if hasattr(object, "utcoffset"):
                                    colonzreplace = _format_offset(object.utcoffset(), sep=":")
                                else:
                                    colonzreplace = ""
                            assert '%' not in colonzreplace
                            newformat.append(colonzreplace)
                        else:
                            push('%')
                            push(ch)
                            push(ch2)
                elif ch == 'Z':
                    if Zreplace is None:
                        Zreplace = ""
                        if hasattr(object, "tzname"):
                            s = object.tzname()
                            if s is not None:
                                # strftime is going to have at this: escape %
                                Zreplace = s.replace('%', '%%')
                    newformat.append(Zreplace)
                # Note that datetime(1000, 1, 1).strftime('%G') == '1000' so
                # year 1000 for %G can go on the fast path.
                elif (ch in 'YGFC' and timetuple[0] < 1000 and
                        _need_normalize_century()):
                    if ch == 'G':
                        year = int(_time.strftime("%G", timetuple))
                    else:
                        year = timetuple[0]
                    if ch == 'C':
                        push('{:02}'.format(year // 100))
                    else:
                        push('{:04}'.format(year))
                        if ch == 'F':
                            push('-{:02}-{:02}'.format(*timetuple[1:3]))
                else:
                    push('%')
                    push(ch)
            else:
                push('%')
        else:
            push(ch)
    newformat = "".join(newformat)
    return _time.strftime(newformat, timetuple)

# tuple[int, int, int] -> tuple[int, int, int] version of date.fromisocalendar
def _isoweek_to_gregorian(year, week, day):
    # Year is bounded this way because 9999-12-31 is (9999, 52, 5)
    if not MINYEAR <= year <= MAXYEAR:
        raise ValueError(f"year must be in {MINYEAR}..{MAXYEAR}, not {year}")

    if not 0 < week < 53:
        out_of_range = True

        if week == 53:
            # ISO years have 53 weeks in them on years starting with a
            # Thursday and leap years starting on a Wednesday
            first_weekday = _ymd2ord(year, 1, 1) % 7
            if (first_weekday == 4 or (first_weekday == 3 and
                                       _is_leap(year))):
                out_of_range = False

        if out_of_range:
            raise ValueError(f"Invalid week: {week}")

    if not 0 < day < 8:
        raise ValueError(f"Invalid weekday: {day} (range is [1, 7])")

    # Now compute the offset from (Y, 1, 1) in days:
    day_offset = (week - 1) * 7 + (day - 1)

    # Calculate the ordinal day for monday, week 1
    day_1 = _isoweek1monday(year)
    ord_day = day_1 + day_offset

    return _ord2ymd(ord_day)


# Just raise TypeError if the arg isn't None or a string.
def _check_tzname(name):
    if name is not None and not isinstance(name, str):
        raise TypeError("tzinfo.tzname() must return None or string, "
                        f"not {type(name).__name__!r}")

# name is the offset-producing method, "utcoffset" or "dst".
# offset is what it returned.
# If offset isn't None or timedelta, raises TypeError.
# If offset is None, returns None.
# Else offset is checked for being in range.
# If it is, its integer value is returned.  Else ValueError is raised.
def _check_utc_offset(name, offset):
    if offset is None:
        return
    if not isinstance(offset, timedelta):
        raise TypeError(f"tzinfo.{name}() must return None "
                        f"or timedelta, not {type(offset).__name__!r}")
    d = offset._days
    if d < -1 or d >= 1 or (d == -1 and offset._seconds == 0 and offset._microseconds < 1):
        raise ValueError("offset must be a timedelta "
                         "strictly between -timedelta(hours=24) and "
                         f"timedelta(hours=24), not {offset!r}")

def _check_date_fields(year, month, day):
    year = _cint(year)
    month = _cint(month)
    day = _cint(day)
    if not MINYEAR <= year <= MAXYEAR:
        raise ValueError(f"year must be in {MINYEAR}..{MAXYEAR}, not {year}")
    if not 1 <= month <= 12:
        raise ValueError(f"month must be in 1..12, not {month}")
    dim = _days_in_month(year, month)
    if not 1 <= day <= dim:
        raise ValueError(f"day {day} must be in range 1..{dim} for month {month} in year {year}")
    return year, month, day

def _check_time_fields(hour, minute, second, microsecond, fold):
    hour = _cint(hour)
    minute = _cint(minute)
    second = _cint(second)
    microsecond = _cint(microsecond)
    fold = _cint(fold)
    if not 0 <= hour <= 23:
        raise ValueError(f"hour must be in 0..23, not {hour}")
    if not 0 <= minute <= 59:
        raise ValueError(f"minute must be in 0..59, not {minute}")
    if not 0 <= second <= 59:
        raise ValueError(f"second must be in 0..59, not {second}")
    if not 0 <= microsecond <= 999999:
        raise ValueError(f"microsecond must be in 0..999999, not {microsecond}")
    if fold not in (0, 1):
        raise ValueError(f"fold must be either 0 or 1, not {fold}")
    return hour, minute, second, microsecond, fold

def _check_tzinfo_arg(tz):
    if tz is not None and not isinstance(tz, tzinfo):
        raise TypeError(
            "tzinfo argument must be None or of a tzinfo subclass, "
            f"not type {type(tz).__name__!r}"
        )

# Objects from fields the native parsers (_mm.dt_iso, _mm.dt_strptime)
# have checked: what the constructors build, without checking again.
def _new_date(cls, year, month, day):
    self = object.__new__(cls)
    self._year = year
    self._month = month
    self._day = day
    self._hashcode = -1
    return self

def _new_time(cls, hour, minute, second, microsecond, tzinfo):
    self = object.__new__(cls)
    self._hour = hour
    self._minute = minute
    self._second = second
    self._microsecond = microsecond
    self._tzinfo = tzinfo
    self._hashcode = -1
    self._fold = 0
    return self

def _new_datetime(cls, year, month, day, hour, minute, second, microsecond, tzinfo):
    self = object.__new__(cls)
    self._year = year
    self._month = month
    self._day = day
    self._hour = hour
    self._minute = minute
    self._second = second
    self._microsecond = microsecond
    self._tzinfo = tzinfo
    self._hashcode = -1
    self._fold = 0
    return self

_DT_DEFAULTS = (None, None, None, (0, 0, 0, 0, None), (0, 0, 0, None), (0, 0, None),
                (0, None), (None,), (), None)

def _td(d, s, us):
    # timedelta(d, s, us) for ints, without the constructor's checks.
    if not 0 <= us < 1000000:
        q, us = divmod(us, 1000000)
        s += q
    if not 0 <= s < 86400:
        q, s = divmod(s, 86400)
        d += q
    if not -999999999 <= d <= 999999999:
        return timedelta(d, s, us)  # the constructor's OverflowError
    self = object.__new__(timedelta)
    self._days = d
    self._seconds = s
    self._microseconds = us
    self._hashcode = -1
    return self

# Aware datetimes as _datetimemodule.c handles them: offsets through
# _utcoff (call_utcoffset), comparisons on the fields (memcmp) or on UTC
# keys (datetime_subtract), arithmetic on ordinals (_mm.dt_ord/dt_ymd).

def _utcoff(dt):
    # datetime_utcoffset: what tzinfo.utcoffset(dt) returns, checked.
    tz = dt._tzinfo
    if tz is None:
        return None
    if tz.__class__ is timezone:
        return tz._offset
    off = tz.utcoffset(dt)
    if off is not None:
        _check_utc_offset("utcoffset", off)
    return off

def _flip_fold(dt):
    # flip_fold: a copy with the other fold, no constructor call.
    o = object.__new__(dt.__class__)
    o._year = dt._year
    o._month = dt._month
    o._day = dt._day
    o._hour = dt._hour
    o._minute = dt._minute
    o._second = dt._second
    o._microsecond = dt._microsecond
    o._tzinfo = dt._tzinfo
    o._hashcode = -1
    o._fold = 0 if dt._fold else 1
    return o

def _off_eq(a, b):
    return a is b or (a._days == b._days and a._seconds == b._seconds and
                      a._microseconds == b._microseconds)

def _pep495(x, y, xoff, yoff):
    # pep495_eq_exception: equal unless an offset depends on fold.
    for dt, off in ((x, xoff), (y, yoff)):
        tz = dt._tzinfo
        if tz is not None and tz.__class__ is not timezone:
            flip = _utcoff(_flip_fold(dt))
            if flip is not off and (flip is None or off is None or not _off_eq(flip, off)):
                return True
    return False

def _fields(dt):
    return (dt._year, dt._month, dt._day, dt._hour, dt._minute, dt._second,
            dt._microsecond)

def _utc_key(dt, off):
    # (seconds since 0001-01-01 UTC, microseconds) of aware dt.
    s = (_dt_ord(dt._year, dt._month, dt._day) * 86400 + dt._hour * 3600 +
         dt._minute * 60 + dt._second - off._days * 86400 - off._seconds)
    us = dt._microsecond - off._microseconds
    if us < 0:
        us += 1000000
        s -= 1
    return (s, us)

def _dt_keys(x, y, eq):
    # datetime_richcompare: keys whose comparison is x's with y's, or None
    # when == is False (naive and aware, or the PEP 495 exception).
    if x._tzinfo is y._tzinfo:
        return (_fields(x), _fields(y))
    xoff = _utcoff(x)
    yoff = _utcoff(y)
    if xoff is yoff or (xoff is not None and yoff is not None and _off_eq(xoff, yoff)):
        k = (_fields(x), _fields(y))
    elif xoff is not None and yoff is not None:
        k = (_utc_key(x, xoff), _utc_key(y, yoff))
    elif eq:
        return None
    else:
        raise TypeError("can't compare offset-naive and offset-aware datetimes")
    if eq and k[0] == k[1] and xoff is not None and _pep495(x, y, xoff, yoff):
        return None
    return k

def _dt_add(dt, dd, ds, dus):
    # add_datetime_timedelta with a normalised delta (dd, ds, dus).
    us = dt._microsecond + dus
    s = dt._hour * 3600 + dt._minute * 60 + dt._second + ds
    if not 0 <= us < 1000000:
        q, us = divmod(us, 1000000)
        s += q
    if not 0 <= s < 86400:
        q, s = divmod(s, 86400)
        dd += q
    n = _dt_ord(dt._year, dt._month, dt._day) + dd
    if not 0 < n <= _MAXORDINAL:
        raise OverflowError("date value out of range")
    y, m, d = _dt_ymd(n)
    h, s = divmod(s, 3600)
    mi, s = divmod(s, 60)
    if dt.__class__ is datetime:
        return _new_datetime(datetime, y, m, d, h, mi, s, us, dt._tzinfo)
    return type(dt)(y, m, d, h, mi, s, us, dt._tzinfo)

_ISO_MIDNIGHT = "minute, second, and microsecond must be 0 when hour is 24"

def _offset_zone(us):
    # timezone(timedelta(microseconds=us)), which raises out of range.
    if not us:
        return timezone.utc
    if not -86400000000 < us < 86400000000:
        return timezone(timedelta(microseconds=us))
    self = object.__new__(timedelta)
    s, self._microseconds = divmod(us, 1000000)
    self._days, self._seconds = divmod(s, 86400)
    self._hashcode = -1
    return timezone._create(self)

def _divide_and_round(a, b):
    """divide a by b and round result to the nearest integer

    When the ratio is exactly half-way between two integers,
    the even integer is returned.
    """
    # Based on the reference implementation for divmod_near
    # in Objects/longobject.c.
    q, r = divmod(a, b)
    # round up if either r / b > 0.5, or r / b == 0.5 and q is odd.
    # The expression r / b > 0.5 is equivalent to 2 * r > b if b is
    # positive, 2 * r < b if b negative.
    r *= 2
    greater_than_half = r > b if b > 0 else r < b
    if greater_than_half or r == b and q % 2 == 1:
        q += 1

    return q


class timedelta:
    """Represent the difference between two datetime objects.

    Supported operators:

    - add, subtract timedelta
    - unary plus, minus, abs
    - compare to timedelta
    - multiply, divide by int

    In addition, datetime supports subtraction of two datetime objects
    returning a timedelta, and addition or subtraction of a datetime
    and a timedelta giving a datetime.

    Representation: (days, seconds, microseconds).
    """
    __mm_tpname__ = 'datetime.timedelta'
    # The representation of (days, seconds, microseconds) was chosen
    # arbitrarily; the exact rationale originally specified in the docstring
    # was "Because I felt like it."


    def __new__(cls, *args, **kwargs):
        days, seconds, microseconds, milliseconds, minutes, hours, weeks = [
            0 if v is _MISSING else v for v in _clinic(
                '__new__', args, kwargs, ('days', 'seconds', 'microseconds', 'milliseconds',
                                          'minutes', 'hours', 'weeks'), 7, 0)]
        if (days.__class__ is int and seconds.__class__ is int and
                microseconds.__class__ is int and milliseconds.__class__ is int and
                minutes.__class__ is int and hours.__class__ is int and weeks.__class__ is int):
            # Ints only: what the general path below computes.
            d = days + weeks * 7
            s = seconds + minutes * 60 + hours * 3600
            us = microseconds + milliseconds * 1000
            if not 0 <= us < 1000000:
                q, us = divmod(us, 1000000)
                s += q
            if not 0 <= s < 86400:
                q, s = divmod(s, 86400)
                d += q
            if -999999999 <= d <= 999999999:
                self = object.__new__(cls)
                self._days = d
                self._seconds = s
                self._microseconds = us
                self._hashcode = -1
                return self
        # Doing this efficiently and accurately in C is going to be difficult
        # and error-prone, due to ubiquitous overflow possibilities, and that
        # C double doesn't have enough bits of precision to represent
        # microseconds over 10K years faithfully.  The code here tries to make
        # explicit where go-fast assumptions can be relied on, in order to
        # guide the C implementation; it's way more convoluted than speed-
        # ignoring auto-overflow-to-long idiomatic Python could be.

        for name, value in (
            ("days", days),
            ("seconds", seconds),
            ("microseconds", microseconds),
            ("milliseconds", milliseconds),
            ("minutes", minutes),
            ("hours", hours),
            ("weeks", weeks)
        ):
            if not isinstance(value, (int, float)):
                raise TypeError(
                    f"unsupported type for timedelta {name} component: {type(value).__name__}"
                )

        # Final values, all integer.
        # s and us fit in 32-bit signed ints; d isn't bounded.
        d = s = us = 0

        # Normalize everything to days, seconds, microseconds.
        days += weeks*7
        seconds += minutes*60 + hours*3600
        microseconds += milliseconds*1000

        # Get rid of all fractions, and normalize s and us.
        # Take a deep breath <wink>.
        if isinstance(days, float):
            dayfrac, days = _math.modf(days)
            daysecondsfrac, daysecondswhole = _math.modf(dayfrac * (24.*3600.))
            assert daysecondswhole == int(daysecondswhole)  # can't overflow
            s = int(daysecondswhole)
            assert days == int(days)
            d = int(days)
        else:
            daysecondsfrac = 0.0
            d = days
        assert isinstance(daysecondsfrac, float)
        assert abs(daysecondsfrac) <= 1.0
        assert isinstance(d, int)
        assert abs(s) <= 24 * 3600
        # days isn't referenced again before redefinition

        if isinstance(seconds, float):
            secondsfrac, seconds = _math.modf(seconds)
            assert seconds == int(seconds)
            seconds = int(seconds)
            secondsfrac += daysecondsfrac
            assert abs(secondsfrac) <= 2.0
        else:
            secondsfrac = daysecondsfrac
        # daysecondsfrac isn't referenced again
        assert isinstance(secondsfrac, float)
        assert abs(secondsfrac) <= 2.0

        assert isinstance(seconds, int)
        days, seconds = divmod(seconds, 24*3600)
        d += days
        s += int(seconds)    # can't overflow
        assert isinstance(s, int)
        assert abs(s) <= 2 * 24 * 3600
        # seconds isn't referenced again before redefinition

        usdouble = secondsfrac * 1e6
        assert abs(usdouble) < 2.1e6    # exact value not critical
        # secondsfrac isn't referenced again

        if isinstance(microseconds, float):
            microseconds = round(microseconds + usdouble)
            seconds, microseconds = divmod(microseconds, 1000000)
            days, seconds = divmod(seconds, 24*3600)
            d += days
            s += seconds
        else:
            microseconds = int(microseconds)
            seconds, microseconds = divmod(microseconds, 1000000)
            days, seconds = divmod(seconds, 24*3600)
            d += days
            s += seconds
            microseconds = round(microseconds + usdouble)
        assert isinstance(s, int)
        assert isinstance(microseconds, int)
        assert abs(s) <= 3 * 24 * 3600
        assert abs(microseconds) < 3.1e6

        # Just a little bit of carrying possible for microseconds and seconds.
        seconds, us = divmod(microseconds, 1000000)
        s += seconds
        days, s = divmod(s, 24*3600)
        d += days

        assert isinstance(d, int)
        assert isinstance(s, int) and 0 <= s < 24*3600
        assert isinstance(us, int) and 0 <= us < 1000000

        if abs(d) > 999999999:
            if not -0x80000000 <= d <= 0x7FFFFFFF:
                raise OverflowError("Python int too large to convert to C int")
            raise OverflowError(f"days={d}; must have magnitude <= 999999999")

        self = object.__new__(cls)
        self._days = d
        self._seconds = s
        self._microseconds = us
        self._hashcode = -1
        return self

    def __repr__(self):
        args = []
        if self._days:
            args.append(f"days={self._days}")
        if self._seconds:
            args.append(f"seconds={self._seconds}")
        if self._microseconds:
            args.append(f"microseconds={self._microseconds}")
        if not args:
            args.append('0')
        return f"{_get_class_module(self)}{self.__class__.__qualname__}({', '.join(args)})"

    def __str__(self):
        mm, ss = divmod(self._seconds, 60)
        hh, mm = divmod(mm, 60)
        s = f"{hh}:{mm:02d}:{ss:02d}"
        if self._days:
            def plural(n):
                return n, abs(n) != 1 and "s" or ""
            s = ("{} day{}, ".format(*plural(self._days))) + s
        if self._microseconds:
            s = s + f".{self._microseconds:06d}"
        return s

    def total_seconds(self):
        """Total seconds in the duration."""
        return ((self.days * 86400 + self.seconds) * 10**6 +
                self.microseconds) / 10**6

    # Read-only field accessors
    @property
    def days(self):
        """days"""
        return self._days

    @property
    def seconds(self):
        """seconds"""
        return self._seconds

    @property
    def microseconds(self):
        """microseconds"""
        return self._microseconds

    def __add__(self, other):
        if isinstance(other, timedelta):
            # for CPython compatibility, we cannot use
            # our __class__ here, but need a real timedelta
            return _td(self._days + other._days,
                       self._seconds + other._seconds,
                       self._microseconds + other._microseconds)
        return NotImplemented

    __radd__ = __add__

    def __sub__(self, other):
        if isinstance(other, timedelta):
            # for CPython compatibility, we cannot use
            # our __class__ here, but need a real timedelta
            return _td(self._days - other._days,
                       self._seconds - other._seconds,
                       self._microseconds - other._microseconds)
        return NotImplemented

    def __rsub__(self, other):
        if isinstance(other, timedelta):
            return -self + other
        return NotImplemented

    def __neg__(self):
        # for CPython compatibility, we cannot use
        # our __class__ here, but need a real timedelta
        return _td(-self._days, -self._seconds, -self._microseconds)

    def __pos__(self):
        return self

    def __abs__(self):
        if self._days < 0:
            return -self
        else:
            return self

    def __mul__(self, other):
        if isinstance(other, int):
            # for CPython compatibility, we cannot use
            # our __class__ here, but need a real timedelta
            return timedelta(self._days * other,
                             self._seconds * other,
                             self._microseconds * other)
        if isinstance(other, float):
            usec = self._to_microseconds()
            a, b = other.as_integer_ratio()
            return timedelta(0, 0, _divide_and_round(usec * a, b))
        return NotImplemented

    __rmul__ = __mul__

    def _to_microseconds(self):
        return ((self._days * (24*3600) + self._seconds) * 1000000 +
                self._microseconds)

    def __floordiv__(self, other):
        if not isinstance(other, (int, timedelta)):
            return NotImplemented
        usec = self._to_microseconds()
        if isinstance(other, timedelta):
            return usec // other._to_microseconds()
        if isinstance(other, int):
            return timedelta(0, 0, usec // other)

    def __truediv__(self, other):
        if not isinstance(other, (int, float, timedelta)):
            return NotImplemented
        usec = self._to_microseconds()
        if isinstance(other, timedelta):
            return usec / other._to_microseconds()
        if isinstance(other, int):
            return timedelta(0, 0, _divide_and_round(usec, other))
        if isinstance(other, float):
            a, b = other.as_integer_ratio()
            return timedelta(0, 0, _divide_and_round(b * usec, a))

    def __mod__(self, other):
        if isinstance(other, timedelta):
            r = self._to_microseconds() % other._to_microseconds()
            return timedelta(0, 0, r)
        return NotImplemented

    def __divmod__(self, other):
        if isinstance(other, timedelta):
            q, r = divmod(self._to_microseconds(),
                          other._to_microseconds())
            return q, timedelta(0, 0, r)
        return NotImplemented

    # Comparisons of timedelta objects with other.

    def __eq__(self, other):
        if isinstance(other, timedelta):
            return self._cmp(other) == 0
        else:
            return NotImplemented

    def __le__(self, other):
        if isinstance(other, timedelta):
            return self._cmp(other) <= 0
        else:
            return NotImplemented

    def __lt__(self, other):
        if isinstance(other, timedelta):
            return self._cmp(other) < 0
        else:
            return NotImplemented

    def __ge__(self, other):
        if isinstance(other, timedelta):
            return self._cmp(other) >= 0
        else:
            return NotImplemented

    def __gt__(self, other):
        if isinstance(other, timedelta):
            return self._cmp(other) > 0
        else:
            return NotImplemented

    def _cmp(self, other):
        assert isinstance(other, timedelta)
        return _cmp(self._getstate(), other._getstate())

    def __hash__(self):
        if self._hashcode == -1:
            self._hashcode = hash(self._getstate())
        return self._hashcode

    def __bool__(self):
        return (self._days != 0 or
                self._seconds != 0 or
                self._microseconds != 0)

    # Pickle support.

    def _getstate(self):
        return (self._days, self._seconds, self._microseconds)

    def __reduce__(self):
        return (self.__class__, self._getstate())

timedelta.min = timedelta(-999999999)
timedelta.max = timedelta(days=999999999, hours=23, minutes=59, seconds=59,
                          microseconds=999999)
timedelta.resolution = timedelta(microseconds=1)

class date:
    """Concrete date type.

    Constructors:

    __new__()
    fromtimestamp()
    today()
    fromordinal()
    strptime()

    Operators:

    __repr__, __str__
    __eq__, __le__, __lt__, __ge__, __gt__, __hash__
    __add__, __radd__, __sub__ (add/radd only with timedelta arg)

    Methods:

    timetuple()
    toordinal()
    weekday()
    isoweekday(), isocalendar(), isoformat()
    ctime()
    strftime()

    Properties (readonly):
    year, month, day
    """
    __mm_tpname__ = 'datetime.date'

    def __new__(cls, *args, **kwargs):
        if len(args) == 1 and not kwargs and isinstance(args[0], bytes):
            year = args[0]
            month = None
        else:
            year, month, day = _getargs(args, kwargs, ('year', 'month', 'day'), 3, 3)
        if (month is None and
            isinstance(year, (bytes, str)) and len(year) == 4 and
            1 <= ord(year[2:3]) <= 12):
            # Pickle support
            if isinstance(year, str):
                try:
                    year = year.encode('latin1')
                except UnicodeEncodeError:
                    # More informative error message.
                    raise ValueError(
                        "Failed to encode latin1 string when unpickling "
                        "a date object. "
                        "pickle.load(data, encoding='latin1') is assumed.")
            self = object.__new__(cls)
            self.__setstate(year)
            self._hashcode = -1
            return self
        year, month, day = _check_date_fields(year, month, day)
        self = object.__new__(cls)
        self._year = year
        self._month = month
        self._day = day
        self._hashcode = -1
        return self

    # Additional constructors

    @classmethod
    def fromtimestamp(cls, t):
        "Construct a date from a POSIX timestamp (like time.time())."
        if t is None:
            raise TypeError("'NoneType' object cannot be interpreted as an integer")
        y, m, d, hh, mm, ss, weekday, jday, dst = _time.localtime(t)
        return cls(y, m, d)

    @classmethod
    def today(cls):
        "Construct a date from time.time()."
        t = _time.time()
        return cls.fromtimestamp(t)

    @classmethod
    def fromordinal(cls, n):
        """Construct a date from a proleptic Gregorian ordinal.

        January 1 of year 1 is day 1.  Only the year, month and day are
        non-zero in the result.
        """
        y, m, d = _ord2ymd(n)
        return cls(y, m, d)

    @classmethod
    def fromisoformat(cls, *args, **kwargs):
        """Construct a date from a string in ISO 8601 format."""
        date_string = _meth_o('date.fromisoformat', args, kwargs)
        if not isinstance(date_string, str):
            raise TypeError('fromisoformat: argument must be str')
        c = _dt_iso(date_string, 0)
        if c.__class__ is int:
            raise ValueError(f'Invalid isoformat string: {date_string!r}')
        y, m, d, ok = c
        return _new_date(cls, y, m, d) if ok and cls is date else cls(y, m, d)

    @classmethod
    def fromisocalendar(cls, year, week, day):
        """Construct a date from the ISO year, week number and weekday.

        This is the inverse of the date.isocalendar() function"""
        return cls(*_isoweek_to_gregorian(year, week, day))

    @classmethod
    def strptime(cls, date_string, format):
        """Parse a date string according to the given format (like time.strptime())."""
        _str_arg('strptime', 1, date_string)
        _str_arg('strptime', 2, format)
        from datetime._strptime import _strptime_datetime_date
        return _strptime_datetime_date(cls, date_string, format)

    # Conversions to string

    def __repr__(self):
        """Convert to formal string, for repr().

        >>> d = date(2010, 1, 1)
        >>> repr(d)
        'datetime.date(2010, 1, 1)'
        """
        return (f"{_get_class_module(self)}{self.__class__.__qualname__}"
                f"({self._year}, {self._month}, {self._day})")
    # XXX These shouldn't depend on time.localtime(), because that
    # clips the usable dates to [1970 .. 2038).  At least ctime() is
    # easily done without using strftime() -- that's better too because
    # strftime("%c", ...) is locale specific.


    def ctime(self):
        "Return ctime() style string."
        weekday = self.toordinal() % 7 or 7
        return (f"{_DAYNAMES[weekday]} {_MONTHNAMES[self._month]} {self._day:2d} "
                f"00:00:00 {self._year:04d}")

    def strftime(self, *args, **kwargs):
        (format,) = _clinic('strftime', args, kwargs, ('format',), 1, 1)
        return _wrap_strftime(self, _str_arg('strftime', 1, format), self.timetuple())

    def __format__(self, fmt):
        if not isinstance(fmt, str):
            raise TypeError(f"must be str, not {type(fmt).__name__}")
        if len(fmt) != 0:
            return self.strftime(fmt)
        return str(self)

    def isoformat(self):
        """Return the date formatted according to ISO.

        This is 'YYYY-MM-DD'.

        References:
        - https://www.w3.org/TR/NOTE-datetime
        - https://www.cl.cam.ac.uk/~mgk25/iso-time.html
        """
        return f"{self._year:04d}-{self._month:02d}-{self._day:02d}"

    __str__ = isoformat

    # Read-only field accessors
    @property
    def year(self):
        """year (1-9999)"""
        return self._year

    @property
    def month(self):
        """month (1-12)"""
        return self._month

    @property
    def day(self):
        """day (1-31)"""
        return self._day

    # Standard conversions, __eq__, __le__, __lt__, __ge__, __gt__,
    # __hash__ (and helpers)

    def timetuple(self):
        "Return local time tuple compatible with time.localtime()."
        return _build_struct_time(self._year, self._month, self._day,
                                  0, 0, 0, -1)

    def toordinal(self):
        """Return proleptic Gregorian ordinal for the year, month and day.

        January 1 of year 1 is day 1.  Only the year, month and day values
        contribute to the result.
        """
        return _ymd2ord(self._year, self._month, self._day)

    def replace(self, *args, **kwargs):
        year, month, day = _clinic('replace', args, kwargs, ('year', 'month', 'day'), 3, 0)
        if year is _MISSING:
            year = self._year
        if month is _MISSING:
            month = self._month
        if day is _MISSING:
            day = self._day
        return type(self)(year, month, day)

    __replace__ = replace

    # Comparisons of date objects with other.

    def __eq__(self, other):
        if isinstance(other, date) and not isinstance(other, datetime):
            return self._cmp(other) == 0
        return NotImplemented

    def __le__(self, other):
        if isinstance(other, date) and not isinstance(other, datetime):
            return self._cmp(other) <= 0
        return NotImplemented

    def __lt__(self, other):
        if isinstance(other, date) and not isinstance(other, datetime):
            return self._cmp(other) < 0
        return NotImplemented

    def __ge__(self, other):
        if isinstance(other, date) and not isinstance(other, datetime):
            return self._cmp(other) >= 0
        return NotImplemented

    def __gt__(self, other):
        if isinstance(other, date) and not isinstance(other, datetime):
            return self._cmp(other) > 0
        return NotImplemented

    def _cmp(self, other):
        assert isinstance(other, date)
        assert not isinstance(other, datetime)
        y, m, d = self._year, self._month, self._day
        y2, m2, d2 = other._year, other._month, other._day
        return _cmp((y, m, d), (y2, m2, d2))

    def __hash__(self):
        "Hash."
        if self._hashcode == -1:
            self._hashcode = hash(self._getstate())
        return self._hashcode

    # Computations

    def __add__(self, other):
        "Add a date to a timedelta."
        if isinstance(other, timedelta):
            o = self.toordinal() + other.days
            if 0 < o <= _MAXORDINAL:
                return type(self).fromordinal(o)
            raise OverflowError("date value out of range")
        return NotImplemented

    __radd__ = __add__

    def __sub__(self, other):
        """Subtract two dates, or a date and a timedelta."""
        if isinstance(other, timedelta):
            return self + timedelta(-other.days)
        if isinstance(other, date):
            days1 = self.toordinal()
            days2 = other.toordinal()
            return timedelta(days1 - days2)
        return NotImplemented

    def weekday(self):
        "Return day of the week, where Monday == 0 ... Sunday == 6."
        return (self.toordinal() + 6) % 7

    # Day-of-the-week and week-of-the-year, according to ISO

    def isoweekday(self):
        "Return day of the week, where Monday == 1 ... Sunday == 7."
        # 1-Jan-0001 is a Monday
        return self.toordinal() % 7 or 7

    def isocalendar(self):
        """Return a named tuple containing ISO year, week number, and weekday.

        The first ISO week of the year is the (Mon-Sun) week
        containing the year's first Thursday; everything else derives
        from that.

        The first week is 1; Monday is 1 ... Sunday is 7.

        ISO calendar algorithm taken from
        https://www.phys.uu.nl/~vgent/calendar/isocalendar.htm
        (used with permission)
        """
        year = self._year
        week1monday = _isoweek1monday(year)
        today = _ymd2ord(self._year, self._month, self._day)
        # Internally, week and day have origin 0
        week, day = divmod(today - week1monday, 7)
        if week < 0:
            year -= 1
            week1monday = _isoweek1monday(year)
            week, day = divmod(today - week1monday, 7)
        elif week >= 52:
            if today >= _isoweek1monday(year+1):
                year += 1
                week = 0
        return _IsoCalendarDate(year, week+1, day+1)

    # Pickle support.

    def _getstate(self):
        yhi, ylo = divmod(self._year, 256)
        return bytes([yhi, ylo, self._month, self._day]),

    def __setstate(self, string):
        yhi, ylo, self._month, self._day = string
        self._year = yhi * 256 + ylo

    def __reduce__(self):
        return (self.__class__, self._getstate())

_date_class = date  # so functions w/ args named "date" can get at the class

date.min = date(1, 1, 1)
date.max = date(9999, 12, 31)
date.resolution = timedelta(days=1)


class tzinfo:
    """Abstract base class for time zone info classes.

    Subclasses must override the tzname(), utcoffset() and dst() methods.
    """
    __mm_tpname__ = 'datetime.tzinfo'

    def tzname(self, dt):
        "datetime -> string name of time zone."
        raise NotImplementedError("a tzinfo subclass must implement tzname()")

    def utcoffset(self, dt):
        "datetime -> timedelta, positive for east of UTC, negative for west of UTC"
        raise NotImplementedError("a tzinfo subclass must implement utcoffset()")

    def dst(self, dt):
        """datetime -> DST offset as timedelta, positive for east of UTC.

        Return 0 if DST not in effect.  utcoffset() must include the DST
        offset.
        """
        raise NotImplementedError("a tzinfo subclass must implement dst()")

    def fromutc(self, dt):
        "datetime in UTC -> datetime in local time."
        # tzinfo_fromutc. See the long comment block at the end of this
        # file for an explanation of this algorithm.
        if not isinstance(dt, datetime):
            raise TypeError("fromutc: argument must be a datetime")
        if dt._tzinfo is not self:
            raise ValueError("fromutc: dt.tzinfo is not self")
        off = _utcoff(dt)
        if off is None:
            raise ValueError("fromutc: non-None utcoffset() result required")
        dst = self.dst(dt)
        _check_utc_offset("dst", dst)
        if dst is None:
            raise ValueError("fromutc: non-None dst() result required")
        delta = _td(off._days - dst._days, off._seconds - dst._seconds,
                    off._microseconds - dst._microseconds)
        dt = _dt_add(dt, delta._days, delta._seconds, delta._microseconds)
        dst = self.dst(dt)
        _check_utc_offset("dst", dst)
        if dst is None:
            raise ValueError("fromutc: tz.dst() gave inconsistent results; cannot convert")
        if dst:
            dt = _dt_add(dt, dst._days, dst._seconds, dst._microseconds)
        return dt

    # Pickle support.

    def __reduce__(self):
        getinitargs = getattr(self, "__getinitargs__", None)
        if getinitargs:
            args = getinitargs()
        else:
            args = ()
        # No `object.__getstate__` in micro-monty: its default, inline.
        getstate = getattr(self, "__getstate__", None)
        if getstate:
            return (self.__class__, args, getstate())
        return (self.__class__, args, getattr(self, "__dict__", None) or None)


class IsoCalendarDate(tuple):
    __mm_tpname__ = 'datetime.IsoCalendarDate'

    def __new__(cls, year, week, weekday, /):
        return super().__new__(cls, (year, week, weekday))

    @property
    def year(self):
        return self[0]

    @property
    def week(self):
        return self[1]

    @property
    def weekday(self):
        return self[2]

    def __reduce__(self):
        # This code is intended to pickle the object without making the
        # class public. See https://bugs.python.org/msg352381
        return (tuple, (tuple(self),))

    def __repr__(self):
        return (f'datetime.IsoCalendarDate'
                f'(year={self[0]}, week={self[1]}, weekday={self[2]})')


_IsoCalendarDate = IsoCalendarDate
del IsoCalendarDate
_tzinfo_class = tzinfo

class time:
    """Time with time zone.

    Constructors:

    __new__()
    strptime()

    Operators:

    __repr__, __str__
    __eq__, __le__, __lt__, __ge__, __gt__, __hash__

    Methods:

    strftime()
    isoformat()
    utcoffset()
    tzname()
    dst()

    Properties (readonly):
    hour, minute, second, microsecond, tzinfo, fold
    """
    __mm_tpname__ = 'datetime.time'

    def __new__(cls, *args, **kwargs):
        hour, minute, second, microsecond, tzinfo, fold = _getargs(
            args, kwargs, ('hour', 'minute', 'second', 'microsecond', 'tzinfo', 'fold'), 5, 0)
        hour = 0 if hour is _MISSING else hour
        minute = 0 if minute is _MISSING else minute
        second = 0 if second is _MISSING else second
        microsecond = 0 if microsecond is _MISSING else microsecond
        tzinfo = None if tzinfo is _MISSING else tzinfo
        fold = 0 if fold is _MISSING else fold
        if (isinstance(hour, bytes) and len(hour) == 6 and
            ord(hour[0:1])&0x7F < 24):
            # Pickle support
            if isinstance(hour, str):
                try:
                    hour = hour.encode('latin1')
                except UnicodeEncodeError:
                    # More informative error message.
                    raise ValueError(
                        "Failed to encode latin1 string when unpickling "
                        "a time object. "
                        "pickle.load(data, encoding='latin1') is assumed.")
            self = object.__new__(cls)
            self.__setstate(hour, minute or None)
            self._hashcode = -1
            return self
        hour, minute, second, microsecond, fold = _check_time_fields(
            hour, minute, second, microsecond, fold)
        _check_tzinfo_arg(tzinfo)
        self = object.__new__(cls)
        self._hour = hour
        self._minute = minute
        self._second = second
        self._microsecond = microsecond
        self._tzinfo = tzinfo
        self._hashcode = -1
        self._fold = fold
        return self

    @classmethod
    def strptime(cls, date_string, format):
        """string, format -> new time parsed from a string (like time.strptime())."""
        _str_arg('strptime', 1, date_string)
        _str_arg('strptime', 2, format)
        from datetime._strptime import _strptime_datetime_time
        return _strptime_datetime_time(cls, date_string, format)

    # Read-only field accessors
    @property
    def hour(self):
        """hour (0-23)"""
        return self._hour

    @property
    def minute(self):
        """minute (0-59)"""
        return self._minute

    @property
    def second(self):
        """second (0-59)"""
        return self._second

    @property
    def microsecond(self):
        """microsecond (0-999999)"""
        return self._microsecond

    @property
    def tzinfo(self):
        """timezone info object"""
        return self._tzinfo

    @property
    def fold(self):
        return self._fold

    # Standard conversions, __hash__ (and helpers)

    # Comparisons of time objects with other.

    def __eq__(self, other):
        if isinstance(other, time):
            return self._cmp(other, allow_mixed=True) == 0
        else:
            return NotImplemented

    def __le__(self, other):
        if isinstance(other, time):
            return self._cmp(other) <= 0
        else:
            return NotImplemented

    def __lt__(self, other):
        if isinstance(other, time):
            return self._cmp(other) < 0
        else:
            return NotImplemented

    def __ge__(self, other):
        if isinstance(other, time):
            return self._cmp(other) >= 0
        else:
            return NotImplemented

    def __gt__(self, other):
        if isinstance(other, time):
            return self._cmp(other) > 0
        else:
            return NotImplemented

    def _cmp(self, other, allow_mixed=False):
        assert isinstance(other, time)
        mytz = self._tzinfo
        ottz = other._tzinfo
        myoff = otoff = None

        if mytz is ottz:
            base_compare = True
        else:
            myoff = self.utcoffset()
            otoff = other.utcoffset()
            base_compare = myoff == otoff

        if base_compare:
            return _cmp((self._hour, self._minute, self._second,
                         self._microsecond),
                        (other._hour, other._minute, other._second,
                         other._microsecond))
        if myoff is None or otoff is None:
            if allow_mixed:
                return 2 # arbitrary non-zero value
            else:
                raise TypeError("can't compare offset-naive and offset-aware times")
        # As C's time_richcompare: whole offset seconds, then microseconds.
        mysecs = (self._hour * 3600 + self._minute * 60 + self._second
                  - myoff.days * 86400 - myoff.seconds)
        otsecs = (other._hour * 3600 + other._minute * 60 + other._second
                  - otoff.days * 86400 - otoff.seconds)
        return _cmp((mysecs, self._microsecond), (otsecs, other._microsecond))

    def __hash__(self):
        """Hash."""
        if self._hashcode == -1:
            if self.fold:
                t = self.replace(fold=0)
            else:
                t = self
            tzoff = t.utcoffset()
            if not tzoff:  # zero or None
                self._hashcode = hash(t._getstate()[0])
            else:
                h, m = divmod(timedelta(hours=self.hour, minutes=self.minute) - tzoff,
                              timedelta(hours=1))
                assert not m % timedelta(minutes=1), "whole minute"
                m //= timedelta(minutes=1)
                if 0 <= h < 24:
                    self._hashcode = hash(time(h, m, self.second, self.microsecond))
                else:
                    self._hashcode = hash((h, m, self.second, self.microsecond))
        return self._hashcode

    # Conversion to string

    def _tzstr(self):
        """Return formatted timezone offset (+xx:xx) or an empty string."""
        off = self.utcoffset()
        return _format_offset(off)

    def __repr__(self):
        """Convert to formal string, for repr()."""
        if self._microsecond != 0:
            s = f", {self._second}, {self._microsecond}"
        elif self._second != 0:
            s = f", {self._second}"
        else:
            s = ""
        s = (f"{_get_class_module(self)}{self.__class__.__qualname__}"
             f"({self._hour}, {self._minute}{s})")
        if self._tzinfo is not None:
            assert s[-1:] == ")"
            s = s[:-1] + f", tzinfo={self._tzinfo!r})"
        if self._fold:
            assert s[-1:] == ")"
            s = s[:-1] + ", fold=1)"
        return s

    def isoformat(self, *args, **kwargs):
        (timespec,) = _clinic('isoformat', args, kwargs, ('timespec',), 1, 0)
        timespec = 'auto' if timespec is _MISSING else _str_arg('isoformat', 1, timespec)
        """Return the time formatted according to ISO.

        The full format is 'HH:MM:SS.mmmmmm+zz:zz'. By default, the fractional
        part is omitted if self.microsecond == 0.

        The optional argument timespec specifies the number of additional
        terms of the time to include. Valid options are 'auto', 'hours',
        'minutes', 'seconds', 'milliseconds' and 'microseconds'.
        """
        s = _format_time(self._hour, self._minute, self._second,
                          self._microsecond, timespec)
        tz = self._tzstr()
        if tz:
            s += tz
        return s

    __str__ = isoformat

    @classmethod
    def fromisoformat(cls, *args, **kwargs):
        """Construct a time from a string in one of the ISO 8601 formats."""
        time_string = _meth_o('time.fromisoformat', args, kwargs)
        if not isinstance(time_string, str):
            raise TypeError('fromisoformat: argument must be str')
        c = _dt_iso(time_string, 1)
        if c.__class__ is int:
            if c:
                raise ValueError(_ISO_MIDNIGHT)
            raise ValueError(f'Invalid isoformat string: {time_string!r}')
        h, m, sec, us, off, ok = c
        tz = None if off is None else _offset_zone(off)
        if ok and cls is time:
            return _new_time(cls, h, m, sec, us, tz)
        return cls(h, m, sec, us, tz)

    def strftime(self, *args, **kwargs):
        (format,) = _clinic('strftime', args, kwargs, ('format',), 1, 1)
        _str_arg('strftime', 1, format)
        # The year must be >= 1000 else Python's strftime implementation
        # can raise a bogus exception.
        timetuple = (1900, 1, 1,
                     self._hour, self._minute, self._second,
                     0, 1, -1)
        return _wrap_strftime(self, format, timetuple)

    def __format__(self, fmt):
        if not isinstance(fmt, str):
            raise TypeError(f"must be str, not {type(fmt).__name__}")
        if len(fmt) != 0:
            return self.strftime(fmt)
        return str(self)

    # Timezone functions

    def utcoffset(self, *args, **kwargs):
        _noargs('time.utcoffset', args, kwargs)
        """Return the timezone offset as timedelta, positive east of UTC
         (negative west of UTC)."""
        if self._tzinfo is None:
            return None
        offset = self._tzinfo.utcoffset(None)
        _check_utc_offset("utcoffset", offset)
        return offset

    def tzname(self, *args, **kwargs):
        _noargs('time.tzname', args, kwargs)
        """Return the timezone name.

        Note that the name is 100% informational -- there's no requirement that
        it mean anything in particular. For example, "GMT", "UTC", "-500",
        "-5:00", "EDT", "US/Eastern", "America/New York" are all valid replies.
        """
        if self._tzinfo is None:
            return None
        name = self._tzinfo.tzname(None)
        _check_tzname(name)
        return name

    def dst(self, *args, **kwargs):
        _noargs('time.dst', args, kwargs)
        """Return 0 if DST is not in effect, or the DST offset (as timedelta
        positive eastward) if DST is in effect.

        This is purely informational; the DST offset has already been added to
        the UTC offset returned by utcoffset() if applicable, so there's no
        need to consult dst() unless you're interested in displaying the DST
        info.
        """
        if self._tzinfo is None:
            return None
        offset = self._tzinfo.dst(None)
        _check_utc_offset("dst", offset)
        return offset

    def replace(self, *args, **kwargs):
        hour, minute, second, microsecond, tzinfo, fold = _clinic(
            'replace', args, kwargs,
            ('hour', 'minute', 'second', 'microsecond', 'tzinfo', 'fold'), 5, 0)
        if hour is _MISSING:
            hour = self.hour
        if minute is _MISSING:
            minute = self.minute
        if second is _MISSING:
            second = self.second
        if microsecond is _MISSING:
            microsecond = self.microsecond
        if tzinfo is _MISSING:
            tzinfo = self.tzinfo
        if fold is _MISSING:
            fold = self._fold
        return type(self)(hour, minute, second, microsecond, tzinfo, fold=fold)

    __replace__ = replace

    # Pickle support.

    def _getstate(self, protocol=3):
        us2, us3 = divmod(self._microsecond, 256)
        us1, us2 = divmod(us2, 256)
        h = self._hour
        if self._fold and protocol > 3:
            h += 128
        basestate = bytes([h, self._minute, self._second,
                           us1, us2, us3])
        if self._tzinfo is None:
            return (basestate,)
        else:
            return (basestate, self._tzinfo)

    def __setstate(self, string, tzinfo):
        if tzinfo is not None and not isinstance(tzinfo, _tzinfo_class):
            raise TypeError("bad tzinfo state arg")
        h, self._minute, self._second, us1, us2, us3 = string
        if h > 127:
            self._fold = 1
            self._hour = h - 128
        else:
            self._fold = 0
            self._hour = h
        self._microsecond = (((us1 << 8) | us2) << 8) | us3
        self._tzinfo = tzinfo

    def __reduce_ex__(self, protocol):
        return (self.__class__, self._getstate(protocol))

    def __reduce__(self):
        return self.__reduce_ex__(2)

_time_class = time  # so functions w/ args named "time" can get at the class

time.min = time(0, 0, 0)
time.max = time(23, 59, 59, 999999)
time.resolution = timedelta(microseconds=1)


class datetime(date):
    """datetime(year, month, day[, hour[, minute[, second[, microsecond[,tzinfo]]]]])

    The year, month and day arguments are required. tzinfo may be None, or an
    instance of a tzinfo subclass. The remaining arguments may be ints.
    """
    __mm_tpname__ = 'datetime.datetime'

    def __new__(cls, *args, **kwargs):
        n = len(args)
        if not kwargs:
            tail = _DT_DEFAULTS[n] if 3 <= n <= 8 else None
        elif 3 <= n <= 7 and len(kwargs) == 1 and 'tzinfo' in kwargs:
            tail = _DT_DEFAULTS[n][:-1] + (kwargs['tzinfo'],)
        else:
            tail = None
        if tail is not None:
            # Ints in range: what the checks below accept.
            y, m, d, h, mi, s, us, tz = args + tail
            if (y.__class__ is int and m.__class__ is int and d.__class__ is int and
                    h.__class__ is int and mi.__class__ is int and s.__class__ is int and
                    us.__class__ is int and 1 <= y <= 9999 and 1 <= m <= 12 and
                    1 <= d <= _DAYS_IN_MONTH[m] + (m == 2 and _is_leap(y)) and
                    0 <= h <= 23 and 0 <= mi <= 59 and 0 <= s <= 59 and
                    0 <= us <= 999999 and (tz is None or isinstance(tz, _tzinfo_class))):
                return _new_datetime(cls, y, m, d, h, mi, s, us, tz)
        if 1 <= len(args) <= 2 and not kwargs and isinstance(args[0], bytes):
            year = args[0]
            month = args[1] if len(args) > 1 else None
        else:
            year, month, day, hour, minute, second, microsecond, tzinfo, fold = _getargs(
                args, kwargs, ('year', 'month', 'day', 'hour', 'minute', 'second',
                               'microsecond', 'tzinfo', 'fold'), 8, 3)
            hour = 0 if hour is _MISSING else hour
            minute = 0 if minute is _MISSING else minute
            second = 0 if second is _MISSING else second
            microsecond = 0 if microsecond is _MISSING else microsecond
            tzinfo = None if tzinfo is _MISSING else tzinfo
            fold = 0 if fold is _MISSING else fold
        if (isinstance(year, bytes) and len(year) == 10 and
            1 <= ord(year[2:3])&0x7F <= 12):
            # Pickle support
            if isinstance(year, str):
                try:
                    year = bytes(year, 'latin1')
                except UnicodeEncodeError:
                    # More informative error message.
                    raise ValueError(
                        "Failed to encode latin1 string when unpickling "
                        "a datetime object. "
                        "pickle.load(data, encoding='latin1') is assumed.")
            self = object.__new__(cls)
            self.__setstate(year, month)
            self._hashcode = -1
            return self
        year, month, day = _check_date_fields(year, month, day)
        hour, minute, second, microsecond, fold = _check_time_fields(
            hour, minute, second, microsecond, fold)
        _check_tzinfo_arg(tzinfo)
        self = object.__new__(cls)
        self._year = year
        self._month = month
        self._day = day
        self._hour = hour
        self._minute = minute
        self._second = second
        self._microsecond = microsecond
        self._tzinfo = tzinfo
        self._hashcode = -1
        self._fold = fold
        return self

    # Read-only field accessors
    @property
    def hour(self):
        """hour (0-23)"""
        return self._hour

    @property
    def minute(self):
        """minute (0-59)"""
        return self._minute

    @property
    def second(self):
        """second (0-59)"""
        return self._second

    @property
    def microsecond(self):
        """microsecond (0-999999)"""
        return self._microsecond

    @property
    def tzinfo(self):
        """timezone info object"""
        return self._tzinfo

    @property
    def fold(self):
        return self._fold

    @classmethod
    def _fromtimestamp(cls, t, utc, tz):
        """Construct a datetime from a POSIX timestamp (like time.time()).

        A timezone info object may be passed in as well.
        """
        frac, t = _math.modf(t)
        us = round(frac * 1e6)
        if us >= 1000000:
            t += 1
            us -= 1000000
        elif us < 0:
            t -= 1
            us += 1000000

        converter = _time.gmtime if utc else _time.localtime
        y, m, d, hh, mm, ss, weekday, jday, dst = converter(t)
        ss = min(ss, 59)    # clamp out leap seconds if the platform has them
        result = cls(y, m, d, hh, mm, ss, us, tz)
        if tz is None and not utc:
            # As of version 2015f max fold in IANA database is
            # 23 hours at 1969-09-30 13:00:00 in Kwajalein.
            # Let's probe 24 hours in the past to detect a transition:
            max_fold_seconds = 24 * 3600

            # On Windows localtime_s throws an OSError for negative values,
            # thus we can't perform fold detection for values of time less
            # than the max time fold. See comments in _datetimemodule's
            # version of this method for more details.
            if False:
                return result

            y, m, d, hh, mm, ss = converter(t - max_fold_seconds)[:6]
            probe1 = cls(y, m, d, hh, mm, ss, us, tz)
            trans = result - probe1 - timedelta(0, max_fold_seconds)
            if trans.days < 0:
                y, m, d, hh, mm, ss = converter(t + trans // timedelta(0, 1))[:6]
                probe2 = cls(y, m, d, hh, mm, ss, us, tz)
                if probe2 == result:
                    result._fold = 1
        elif tz is not None:
            result = tz.fromutc(result)
        return result

    @classmethod
    def fromtimestamp(cls, timestamp, tz=None):
        """Construct a datetime from a POSIX timestamp (like time.time()).

        A timezone info object may be passed in as well.
        """
        _check_tzinfo_arg(tz)

        return cls._fromtimestamp(timestamp, tz is not None, tz)

    @classmethod
    def utcfromtimestamp(cls, t):
        """Construct a naive UTC datetime from a POSIX timestamp."""
        import warnings
        warnings.warn("datetime.datetime.utcfromtimestamp() is deprecated and scheduled "
                      "for removal in a future version. Use timezone-aware "
                      "objects to represent datetimes in UTC: "
                      "datetime.datetime.fromtimestamp(timestamp, datetime.UTC).",
                      DeprecationWarning,
                      stacklevel=2)
        return cls._fromtimestamp(t, True, None)

    @classmethod
    def now(cls, *args, **kwargs):
        (tz,) = _clinic('now', args, kwargs, ('tz',), 1, 0)
        tz = None if tz is _MISSING else tz
        t = _time.time()
        return cls.fromtimestamp(t, tz)

    @classmethod
    def utcnow(cls):
        "Construct a UTC datetime from time.time()."
        import warnings
        warnings.warn("datetime.datetime.utcnow() is deprecated and scheduled for "
                      "removal in a future version. Use timezone-aware "
                      "objects to represent datetimes in UTC: "
                      "datetime.datetime.now(datetime.UTC).",
                      DeprecationWarning,
                      stacklevel=2)
        t = _time.time()
        return cls._fromtimestamp(t, True, None)

    @classmethod
    def combine(cls, *args, **kwargs):
        date, time, tzinfo = _clinic('combine', args, kwargs, ('date', 'time', 'tzinfo'), 3, 2)
        if not isinstance(date, _date_class):
            raise TypeError("combine() argument 1 must be datetime.date, not "
                            f"{type(date).__name__}")
        if not isinstance(time, _time_class):
            raise TypeError("combine() argument 2 must be datetime.time, not "
                            f"{type(time).__name__}")
        if tzinfo is _MISSING:
            tzinfo = time.tzinfo
        return cls(date.year, date.month, date.day,
                   time.hour, time.minute, time.second, time.microsecond,
                   tzinfo, fold=time.fold)

    @classmethod
    def fromisoformat(cls, *args, **kwargs):
        """Construct a datetime from a string in one of the ISO 8601 formats."""
        date_string = _meth_o('datetime.fromisoformat', args, kwargs)
        if not isinstance(date_string, str):
            raise TypeError('fromisoformat: argument must be str')
        c = _dt_iso(date_string, 2) if len(date_string) >= 7 else 0
        if c.__class__ is int:
            raise ValueError(f'Invalid isoformat string: {date_string!r}')
        y, mo, d, h, m, sec, us, off, ok = c
        tz = None if off is None else _offset_zone(off)
        if ok == 2:
            raise ValueError(_ISO_MIDNIGHT)
        if ok and cls is datetime:
            return _new_datetime(cls, y, mo, d, h, m, sec, us, tz)
        return cls(y, mo, d, h, m, sec, us, tz)

    def timetuple(self):
        "Return local time tuple compatible with time.localtime()."
        dst = self.dst()
        if dst is None:
            dst = -1
        elif dst:
            dst = 1
        else:
            dst = 0
        return _build_struct_time(self.year, self.month, self.day,
                                  self.hour, self.minute, self.second,
                                  dst)

    def _mktime(self):
        """Return integer POSIX timestamp."""
        epoch = datetime(1970, 1, 1)
        max_fold_seconds = 24 * 3600
        t = (self - epoch) // timedelta(0, 1)
        def local(u):
            y, m, d, hh, mm, ss = _time.localtime(u)[:6]
            return (datetime(y, m, d, hh, mm, ss) - epoch) // timedelta(0, 1)

        # Our goal is to solve t = local(u) for u.
        a = local(t) - t
        u1 = t - a
        t1 = local(u1)
        if t1 == t:
            # We found one solution, but it may not be the one we need.
            # Look for an earlier solution (if `fold` is 0), or a
            # later one (if `fold` is 1).
            u2 = u1 + (-max_fold_seconds, max_fold_seconds)[self.fold]
            b = local(u2) - u2
            if a == b:
                return u1
        else:
            b = t1 - u1
            assert a != b
        u2 = t - b
        t2 = local(u2)
        if t2 == t:
            return u2
        if t1 == t:
            return u1
        # We have found both offsets a and b, but neither t - a nor t - b is
        # a solution.  This means t is in the gap.
        return (max, min)[self.fold](u1, u2)


    def timestamp(self):
        "Return POSIX timestamp as float"
        if self._tzinfo is None:
            s = self._mktime()
            return s + self.microsecond / 1e6
        else:
            return (self - _EPOCH).total_seconds()

    def utctimetuple(self):
        "Return UTC time tuple compatible with time.gmtime()."
        offset = self.utcoffset()
        if offset:
            self -= offset
        y, m, d = self.year, self.month, self.day
        hh, mm, ss = self.hour, self.minute, self.second
        return _build_struct_time(y, m, d, hh, mm, ss, 0)

    def date(self, *args, **kwargs):
        _noargs('datetime.date', args, kwargs)
        "Return the date part."
        return date(self._year, self._month, self._day)

    def time(self, *args, **kwargs):
        _noargs('datetime.time', args, kwargs)
        "Return the time part, with tzinfo None."
        return time(self.hour, self.minute, self.second, self.microsecond, fold=self.fold)

    def timetz(self, *args, **kwargs):
        _noargs('datetime.timetz', args, kwargs)
        "Return the time part, with same tzinfo."
        return time(self.hour, self.minute, self.second, self.microsecond,
                    self._tzinfo, fold=self.fold)

    def replace(self, *args, **kwargs):
        year, month, day, hour, minute, second, microsecond, tzinfo, fold = _clinic(
            'replace', args, kwargs, ('year', 'month', 'day', 'hour', 'minute', 'second',
                                      'microsecond', 'tzinfo', 'fold'), 8, 0)
        if year is _MISSING:
            year = self.year
        if month is _MISSING:
            month = self.month
        if day is _MISSING:
            day = self.day
        if hour is _MISSING:
            hour = self.hour
        if minute is _MISSING:
            minute = self.minute
        if second is _MISSING:
            second = self.second
        if microsecond is _MISSING:
            microsecond = self.microsecond
        if tzinfo is _MISSING:
            tzinfo = self.tzinfo
        if fold is _MISSING:
            fold = self.fold
        return type(self)(year, month, day, hour, minute, second,
                          microsecond, tzinfo, fold=fold)

    __replace__ = replace

    def _local_timezone(self):
        # local_timezone_from_local when naive or without an offset.
        if self._tzinfo is None or _utcoff(self) is None:
            ts = self._mktime()
            # Detect gap
            ts2 = self.replace(fold=1-self.fold)._mktime()
            if ts2 != ts: # This happens in a gap or a fold
                if (ts2 > ts) == self.fold:
                    ts = ts2
        else:
            ts = (self - _EPOCH) // timedelta(seconds=1)
        localtm = _time.localtime(ts)
        local = datetime(*localtm[:6])
        # Extract TZ data
        gmtoff = localtm.tm_gmtoff
        zone = localtm.tm_zone
        return timezone(timedelta(seconds=gmtoff), zone)

    def astimezone(self, *args, **kwargs):
        (tz,) = _clinic('astimezone', args, kwargs, ('tz',), 1, 0)
        if tz is _MISSING or tz is None:
            tz = self._local_timezone()
        else:
            _check_tzinfo_arg(tz)
            mytz = self._tzinfo
            if mytz is not None:
                # datetime_astimezone for an aware self.
                if mytz is tz:
                    return self
                off = _utcoff(self)
                if off is not None:
                    utc = _dt_add(self, -off._days, -off._seconds, -off._microseconds)
                    utc._tzinfo = tz
                    if tz.__class__ is timezone:
                        off = tz._offset
                        return _dt_add(utc, off._days, off._seconds, off._microseconds)
                    return tz.fromutc(utc)

        mytz = self.tzinfo
        if mytz is None:
            mytz = self._local_timezone()
            myoffset = mytz.utcoffset(self)
        else:
            myoffset = mytz.utcoffset(self)
            if myoffset is None:
                mytz = self.replace(tzinfo=None)._local_timezone()
                myoffset = mytz.utcoffset(self)

        if tz is mytz:
            return self

        # Convert self to UTC, and attach the new time zone object.
        utc = (self - myoffset).replace(tzinfo=tz)

        # Convert from UTC to tz's local time.
        return tz.fromutc(utc)

    # Ways to produce a string.

    def ctime(self):
        "Return ctime() style string."
        weekday = self.toordinal() % 7 or 7
        return (f"{_DAYNAMES[weekday]} {_MONTHNAMES[self._month]} {self._day:2d} "
                f"{self._hour:02d}:{self._minute:02d}:{self._second:02d} {self._year:04d}")

    def isoformat(self, *args, **kwargs):
        sep, timespec = _clinic('isoformat', args, kwargs, ('sep', 'timespec'), 2, 0)
        if sep is _MISSING:
            sep = 'T'
        elif not isinstance(sep, str) or len(sep) != 1:
            got = ('None' if sep is None else type(sep).__name__ if not isinstance(sep, str)
                   else f'a string of length {len(sep)}')
            raise TypeError(f'isoformat() argument 1 must be a unicode character, not {got}')
        timespec = 'auto' if timespec is _MISSING else _str_arg('isoformat', 2, timespec)
        """Return the time formatted according to ISO.

        The full format looks like 'YYYY-MM-DD HH:MM:SS.mmmmmm'.
        By default, the fractional part is omitted if self.microsecond == 0.

        If self.tzinfo is not None, the UTC offset is also attached, giving
        a full format of 'YYYY-MM-DD HH:MM:SS.mmmmmm+HH:MM'.

        Optional argument sep specifies the separator between date and
        time, default 'T'.

        The optional argument timespec specifies the number of additional
        terms of the time to include. Valid options are 'auto', 'hours',
        'minutes', 'seconds', 'milliseconds' and 'microseconds'.
        """
        s = (f"{self._year:04d}-{self._month:02d}-{self._day:02d}{sep}" +
             _format_time(self._hour, self._minute, self._second,
                          self._microsecond, timespec))

        if self._tzinfo is not None:
            s += _format_offset(_utcoff(self))
        return s

    def __repr__(self):
        """Convert to formal string, for repr()."""
        L = [self._year, self._month, self._day,  # These are never zero
             self._hour, self._minute, self._second, self._microsecond]
        if L[-1] == 0:
            del L[-1]
        if L[-1] == 0:
            del L[-1]
        s = (f"{_get_class_module(self)}{self.__class__.__qualname__}"
             f"({', '.join(map(str, L))})")
        # datetime_repr: fold, then tzinfo.
        if self._fold:
            s = s[:-1] + ", fold=1)"
        if self._tzinfo is not None:
            s = s[:-1] + f", tzinfo={self._tzinfo!r})"
        return s

    def __str__(self):
        "Convert to string, for str()."
        return self.isoformat(sep=' ')

    @classmethod
    def strptime(cls, date_string, format):
        'string, format -> new datetime parsed from a string (like time.strptime()).'
        _str_arg('strptime', 1, date_string)
        _str_arg('strptime', 2, format)
        from datetime._strptime import _strptime_datetime_datetime
        return _strptime_datetime_datetime(cls, date_string, format)

    def utcoffset(self, *args, **kwargs):
        """Return the timezone offset as timedelta positive east of UTC (negative west of
        UTC)."""
        if args or kwargs:
            _noargs('datetime.utcoffset', args, kwargs)
        return _utcoff(self)

    def tzname(self, *args, **kwargs):
        _noargs('datetime.tzname', args, kwargs)
        """Return the timezone name.

        Note that the name is 100% informational -- there's no requirement that
        it mean anything in particular. For example, "GMT", "UTC", "-500",
        "-5:00", "EDT", "US/Eastern", "America/New York" are all valid replies.
        """
        if self._tzinfo is None:
            return None
        name = self._tzinfo.tzname(self)
        _check_tzname(name)
        return name

    def dst(self, *args, **kwargs):
        _noargs('datetime.dst', args, kwargs)
        """Return 0 if DST is not in effect, or the DST offset (as timedelta
        positive eastward) if DST is in effect.

        This is purely informational; the DST offset has already been added to
        the UTC offset returned by utcoffset() if applicable, so there's no
        need to consult dst() unless you're interested in displaying the DST
        info.
        """
        if self._tzinfo is None:
            return None
        offset = self._tzinfo.dst(self)
        _check_utc_offset("dst", offset)
        return offset

    # Comparisons of datetime objects with other.

    def __eq__(self, other):
        if isinstance(other, datetime):
            if self._tzinfo is other._tzinfo:
                return ((self._year, self._month, self._day, self._hour, self._minute,
                         self._second, self._microsecond) ==
                        (other._year, other._month, other._day, other._hour, other._minute,
                         other._second, other._microsecond))
            k = _dt_keys(self, other, True)
            return k is not None and k[0] == k[1]
        else:
            return NotImplemented

    def __le__(self, other):
        if isinstance(other, datetime):
            if self._tzinfo is other._tzinfo:
                return ((self._year, self._month, self._day, self._hour, self._minute,
                         self._second, self._microsecond) <=
                        (other._year, other._month, other._day, other._hour, other._minute,
                         other._second, other._microsecond))
            k = _dt_keys(self, other, False)
            return k[0] <= k[1]
        else:
            return NotImplemented

    def __lt__(self, other):
        if isinstance(other, datetime):
            if self._tzinfo is other._tzinfo:
                return ((self._year, self._month, self._day, self._hour, self._minute,
                         self._second, self._microsecond) <
                        (other._year, other._month, other._day, other._hour, other._minute,
                         other._second, other._microsecond))
            k = _dt_keys(self, other, False)
            return k[0] < k[1]
        else:
            return NotImplemented

    def __ge__(self, other):
        if isinstance(other, datetime):
            if self._tzinfo is other._tzinfo:
                return ((self._year, self._month, self._day, self._hour, self._minute,
                         self._second, self._microsecond) >=
                        (other._year, other._month, other._day, other._hour, other._minute,
                         other._second, other._microsecond))
            k = _dt_keys(self, other, False)
            return k[0] >= k[1]
        else:
            return NotImplemented

    def __gt__(self, other):
        if isinstance(other, datetime):
            if self._tzinfo is other._tzinfo:
                return ((self._year, self._month, self._day, self._hour, self._minute,
                         self._second, self._microsecond) >
                        (other._year, other._month, other._day, other._hour, other._minute,
                         other._second, other._microsecond))
            k = _dt_keys(self, other, False)
            return k[0] > k[1]
        else:
            return NotImplemented

    def __add__(self, other):
        "Add a datetime and a timedelta."
        if not isinstance(other, timedelta):
            return NotImplemented
        return _dt_add(self, other._days, other._seconds, other._microseconds)

    __radd__ = __add__

    def __sub__(self, other):
        "Subtract two datetimes, or a datetime and a timedelta."
        if not isinstance(other, datetime):
            if isinstance(other, timedelta):
                return _dt_add(self, -other._days, -other._seconds, -other._microseconds)
            return NotImplemented
        d = (_dt_ord(self._year, self._month, self._day) -
             _dt_ord(other._year, other._month, other._day))
        s = ((self._hour - other._hour) * 3600 + (self._minute - other._minute) * 60 +
             self._second - other._second)
        us = self._microsecond - other._microsecond
        if self._tzinfo is not other._tzinfo:
            myoff = _utcoff(self)
            otoff = _utcoff(other)
            if (myoff is None) != (otoff is None):
                raise TypeError("can't subtract offset-naive and offset-aware datetimes")
            if myoff is not None and not _off_eq(myoff, otoff):
                d += otoff._days - myoff._days
                s += otoff._seconds - myoff._seconds
                us += otoff._microseconds - myoff._microseconds
        return _td(d, s, us)

    def __hash__(self):
        if self._hashcode == -1:
            tzoff = _utcoff(_flip_fold(self) if self._fold else self)
            if tzoff is None:
                self._hashcode = hash(self._getstate()[0])
            else:
                s, us = _utc_key(self, tzoff)
                self._hashcode = hash((*divmod(s, 86400), us))
        return self._hashcode

    # Pickle support.

    def _getstate(self, protocol=3):
        yhi, ylo = divmod(self._year, 256)
        us2, us3 = divmod(self._microsecond, 256)
        us1, us2 = divmod(us2, 256)
        m = self._month
        if self._fold and protocol > 3:
            m += 128
        basestate = bytes([yhi, ylo, m, self._day,
                           self._hour, self._minute, self._second,
                           us1, us2, us3])
        if self._tzinfo is None:
            return (basestate,)
        else:
            return (basestate, self._tzinfo)

    def __setstate(self, string, tzinfo):
        if tzinfo is not None and not isinstance(tzinfo, _tzinfo_class):
            raise TypeError("bad tzinfo state arg")
        (yhi, ylo, m, self._day, self._hour,
         self._minute, self._second, us1, us2, us3) = string
        if m > 127:
            self._fold = 1
            self._month = m - 128
        else:
            self._fold = 0
            self._month = m
        self._year = yhi * 256 + ylo
        self._microsecond = (((us1 << 8) | us2) << 8) | us3
        self._tzinfo = tzinfo

    def __reduce_ex__(self, protocol):
        return (self.__class__, self._getstate(protocol))

    def __reduce__(self):
        return self.__reduce_ex__(2)


datetime.min = datetime(1, 1, 1)
datetime.max = datetime(9999, 12, 31, 23, 59, 59, 999999)
datetime.resolution = timedelta(microseconds=1)


def _isoweek1monday(year):
    # Helper to calculate the day number of the Monday starting week 1
    THURSDAY = 3
    firstday = _ymd2ord(year, 1, 1)
    firstweekday = (firstday + 6) % 7  # See weekday() above
    week1monday = firstday - firstweekday
    if firstweekday > THURSDAY:
        week1monday += 7
    return week1monday


class timezone(tzinfo):
    __mm_tpname__ = 'datetime.timezone'

    # Sentinel value to disallow None
    _Omitted = object()
    def __new__(cls, *args, **kwargs):
        offset, name = _clinic('timezone', args, kwargs, ('offset', 'name'), 2, 1)
        if not isinstance(offset, timedelta):
            raise TypeError("timezone() argument 1 must be datetime.timedelta, not "
                            f"{'None' if offset is None else type(offset).__name__}")
        if name is _MISSING:
            if not offset:
                return cls.utc
            name = None
        else:
            _str_arg('timezone', 2, name)
        if not cls._minoffset <= offset <= cls._maxoffset:
            raise ValueError("offset must be a timedelta "
                             "strictly between -timedelta(hours=24) and "
                             f"timedelta(hours=24), not {offset!r}")
        return cls._create(offset, name)

    def __init_subclass__(cls):
        raise TypeError("type 'datetime.timezone' is not an acceptable base type")

    @classmethod
    def _create(cls, offset, name=None):
        self = tzinfo.__new__(cls)
        self._offset = offset
        self._name = name
        return self

    def __getinitargs__(self):
        """pickle support"""
        if self._name is None:
            return (self._offset,)
        return (self._offset, self._name)

    def __eq__(self, other):
        if isinstance(other, timezone):
            return self._offset == other._offset
        return NotImplemented

    def __hash__(self):
        return hash(self._offset)

    def __repr__(self):
        """Convert to formal string, for repr().

        >>> tz = timezone.utc
        >>> repr(tz)
        'datetime.timezone.utc'
        >>> tz = timezone(timedelta(hours=-5), 'EST')
        >>> repr(tz)
        "datetime.timezone(datetime.timedelta(-1, 68400), 'EST')"
        """
        if self is self.utc:
            return 'datetime.timezone.utc'
        if self._name is None:
            return (f"{_get_class_module(self)}{self.__class__.__qualname__}"
                    f"({self._offset!r})")
        return (f"{_get_class_module(self)}{self.__class__.__qualname__}"
                f"({self._offset!r}, {self._name!r})")

    def __str__(self):
        return self.tzname(None)

    def utcoffset(self, *args, **kwargs):
        dt = _meth_o('timezone.utcoffset', args, kwargs)
        if not isinstance(dt, datetime) and dt is not None:
            raise TypeError("utcoffset(dt) argument must be a datetime instance or None, not "
                            f"{type(dt).__name__}")
        if True:
            return self._offset
        raise TypeError("utcoffset() argument must be a datetime instance"
                        " or None")

    def tzname(self, *args, **kwargs):
        dt = _meth_o('timezone.tzname', args, kwargs)
        if not isinstance(dt, datetime) and dt is not None:
            raise TypeError("tzname(dt) argument must be a datetime instance or None, not "
                            f"{type(dt).__name__}")
        if True:
            if self._name is None:
                return self._name_from_offset(self._offset)
            return self._name
        raise TypeError("tzname() argument must be a datetime instance"
                        " or None")

    def dst(self, *args, **kwargs):
        dt = _meth_o('timezone.dst', args, kwargs)
        if not isinstance(dt, datetime) and dt is not None:
            raise TypeError("dst(dt) argument must be a datetime instance or None, not "
                            f"{type(dt).__name__}")
        if True:
            return None
        raise TypeError("dst() argument must be a datetime instance"
                        " or None")

    def fromutc(self, dt):
        if isinstance(dt, datetime):
            if dt.tzinfo is not self:
                raise ValueError("fromutc: dt.tzinfo "
                                 "is not self")
            off = self._offset
            return _dt_add(dt, off._days, off._seconds, off._microseconds)
        raise TypeError("fromutc() argument must be a datetime instance"
                        " or None")

    _maxoffset = timedelta(hours=24, microseconds=-1)
    _minoffset = -_maxoffset

    @staticmethod
    def _name_from_offset(delta):
        if not delta:
            return 'UTC'
        if delta < timedelta(0):
            sign = '-'
            delta = -delta
        else:
            sign = '+'
        hours, rest = divmod(delta, timedelta(hours=1))
        minutes, rest = divmod(rest, timedelta(minutes=1))
        seconds = rest.seconds
        microseconds = rest.microseconds
        if microseconds:
            return (f'UTC{sign}{hours:02d}:{minutes:02d}:{seconds:02d}'
                    f'.{microseconds:06d}')
        if seconds:
            return f'UTC{sign}{hours:02d}:{minutes:02d}:{seconds:02d}'
        return f'UTC{sign}{hours:02d}:{minutes:02d}'

UTC = timezone.utc = timezone._create(timedelta(0))

# bpo-37642: These attributes are rounded to the nearest minute for backwards
# compatibility, even though the constructor will accept a wider range of
# values. This may change in the future.
timezone.min = timezone._create(-timedelta(hours=23, minutes=59))
timezone.max = timezone._create(timedelta(hours=23, minutes=59))
_EPOCH = datetime(1970, 1, 1, tzinfo=timezone.utc)
