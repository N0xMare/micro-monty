# strptime for `time` and `datetime`: CPython 3.14's Lib/_strptime.py with
# the C locale's names fixed in place of LocaleTime (no locale module), and
# the zone names of the time module's (UTC) zone. PSF license; see
# THIRD_PARTY.md.

import time
from re import compile as re_compile, sub as re_sub, escape as re_escape, IGNORECASE
from datetime import (date as datetime_date, timedelta as datetime_timedelta,
                      timezone as datetime_timezone, datetime as datetime_datetime,
                      _new_datetime, _offset_zone)
from _mm import dt_strptime as _dt_strptime

_F_WEEKDAY = ['monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday', 'sunday']
_A_WEEKDAY = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun']
_F_MONTH = ['', 'january', 'february', 'march', 'april', 'may', 'june', 'july', 'august',
            'september', 'october', 'november', 'december']
_A_MONTH = ['', 'jan', 'feb', 'mar', 'apr', 'may', 'jun', 'jul', 'aug', 'sep', 'oct', 'nov',
            'dec']
_AM_PM = ['am', 'pm']
_TIMEZONE = (('utc', 'gmt', time.tzname[0].lower()),
             (time.tzname[1].lower(),) if time.daylight else ())


def _seq_to_re(to_convert, directive):
    to_convert = sorted(to_convert, key=len, reverse=True)
    for value in to_convert:
        if value != '':
            break
    else:
        return ''
    regex = '|'.join([re_escape(stuff) for stuff in to_convert])
    return f'(?P<{directive}>{regex})'


_MAPPING = {
    'd': r"(?P<d>3[0-1]|[1-2]\d|0[1-9]|[1-9]| [1-9])",
    'f': r"(?P<f>[0-9]{1,6})",
    'H': r"(?P<H>2[0-3]|[0-1]\d|\d| \d)",
    'k': r"(?P<H>2[0-3]|[0-1]\d|\d| \d)",
    'I': r"(?P<I>1[0-2]|0[1-9]|[1-9]| [1-9])",
    'l': r"(?P<I>1[0-2]|0[1-9]|[1-9]| [1-9])",
    'G': r"(?P<G>\d\d\d\d)",
    'j': r"(?P<j>36[0-6]|3[0-5]\d|[1-2]\d\d|0[1-9]\d|00[1-9]|[1-9]\d|0[1-9]|[1-9])",
    'm': r"(?P<m>1[0-2]|0[1-9]|[1-9])",
    'M': r"(?P<M>[0-5]\d|\d)",
    'S': r"(?P<S>6[0-1]|[0-5]\d|\d)",
    'U': r"(?P<U>5[0-3]|[0-4]\d|\d)",
    'w': r"(?P<w>[0-6])",
    'u': r"(?P<u>[1-7])",
    'V': r"(?P<V>5[0-3]|0[1-9]|[1-4]\d|\d)",
    'y': r"(?P<y>\d\d)",
    'Y': r"(?P<Y>\d\d\d\d)",
    'z': r"(?P<z>[+-]\d\d:?[0-5]\d(:?[0-5]\d(\.\d{1,6})?)?|(?-i:Z))",
    'A': _seq_to_re(_F_WEEKDAY, 'A'),
    'a': _seq_to_re(_A_WEEKDAY, 'a'),
    'B': _seq_to_re(_F_MONTH[1:], 'B'),
    'b': _seq_to_re(_A_MONTH[1:], 'b'),
    'p': _seq_to_re(_AM_PM, 'p'),
    'Z': _seq_to_re([tz for tz_names in _TIMEZONE for tz in tz_names], 'Z'),
    '%': '%'}
for _d in 'dmyCHIMS':
    _MAPPING['O' + _d] = f'(?P<{_d}>\\d\\d|\\d| \\d)'
_MAPPING['Ow'] = r'(?P<w>\d)'
_MAPPING['e'] = _MAPPING['d']
_MAPPING['Oe'] = _MAPPING['Od']
_MAPPING['P'] = _MAPPING['p']
_MAPPING['Op'] = _MAPPING['p']
_MAPPING['W'] = _MAPPING['U'].replace('U', 'W')


def _pattern(format):
    format = re_sub(r"([\\.^$*+?\(\){}\[\]|])", r"\\\1", format)
    format = re_sub(r'\s+', r'\\s+', format)
    format = re_sub(r"'", "['\u02bc]", format)
    seen = set()

    def repl(m):
        seen.add(m[1])
        return _MAPPING[m[1]]
    format = re_sub(r'%[-_0^#]*[0-9]*([OE]?\\?.?)', repl, format)
    if 'd' in seen and not seen & {'Y', 'y', 'G'}:
        import warnings
        warnings.warn("""\
Parsing dates involving a day of month without a year specified is ambiguous
and fails to parse leap day. The default behavior will change in Python 3.15
to either always raise an exception or to use a different default year (TBD).
To avoid trouble, add a specific year to the input & format.
See https://github.com/python/cpython/issues/70647.""",
                      DeprecationWarning,
                      skip_file_prefixes=("<datetime",))
    return format


_MAPPING['T'] = _pattern('%H:%M:%S')
_MAPPING['R'] = _pattern('%H:%M')
_MAPPING['r'] = _pattern('%I:%M:%S %p')
_MAPPING['X'] = _pattern('%H:%M:%S')
_MAPPING['x'] = _pattern('%m/%d/%y')
_MAPPING['c'] = _pattern('%a %b %d %H:%M:%S %Y')

_CACHE_MAX_SIZE = 5
_regex_cache = {}


def _calc_julian_from_U_or_W(year, week_of_year, day_of_week, week_starts_Mon):
    first_weekday = datetime_date(year, 1, 1).weekday()
    if not week_starts_Mon:
        first_weekday = (first_weekday + 1) % 7
        day_of_week = (day_of_week + 1) % 7
    week_0_length = (7 - first_weekday) % 7
    if week_of_year == 0:
        return 1 + day_of_week - first_weekday
    else:
        days_to_week = week_0_length + (7 * (week_of_year - 1))
        return 1 + days_to_week + day_of_week


def _strptime(data_string, format="%a %b %d %H:%M:%S %Y"):
    # The common directives natively (_mm.dt_strptime gives the same result
    # or None); everything else, and every error, takes the path below.
    if (r := _dt_strptime(data_string, format)) is not None:
        return r
    for index, arg in enumerate([data_string, format]):
        if not isinstance(arg, str):
            raise TypeError(f"strptime() argument {index} must be str, not {type(arg)}")
    if len(_regex_cache) > _CACHE_MAX_SIZE:
        _regex_cache.clear()
    format_regex = _regex_cache.get(format)
    if not format_regex:
        try:
            format_regex = re_compile(_pattern(format), IGNORECASE)
        except KeyError as err:
            bad_directive = err.args[0]
            bad_directive = bad_directive.replace('\\s', '')
            if not bad_directive:
                raise ValueError(f"stray % in format '{format}'") from None
            bad_directive = bad_directive.replace('\\', '', 1)
            raise ValueError(f"'{bad_directive}' is a bad directive in format "
                             f"'{format}'") from None
        _regex_cache[format] = format_regex
    found = format_regex.match(data_string)
    if not found:
        raise ValueError(f"time data {data_string!r} does not match format {format!r}")
    if len(data_string) != found.end():
        raise ValueError(f"unconverted data remains: {data_string[found.end():]}")

    iso_year = year = None
    month = day = 1
    hour = minute = second = fraction = 0
    tz = -1
    gmtoff = None
    gmtoff_fraction = 0
    iso_week = week_of_year = None
    week_of_year_start = None
    weekday = julian = None
    found_dict = found.groupdict()
    for group_key in found_dict.keys():
        if group_key == 'y':
            year = int(found_dict['y'])
            if 'C' in found_dict:
                century = int(found_dict['C'])
                year += century * 100
            else:
                if year <= 68:
                    year += 2000
                else:
                    year += 1900
        elif group_key == 'Y':
            year = int(found_dict['Y'])
        elif group_key == 'G':
            iso_year = int(found_dict['G'])
        elif group_key == 'm':
            month = int(found_dict['m'])
        elif group_key == 'B':
            month = _F_MONTH.index(found_dict['B'].lower())
        elif group_key == 'b':
            month = _A_MONTH.index(found_dict['b'].lower())
        elif group_key == 'd':
            day = int(found_dict['d'])
        elif group_key == 'H':
            hour = int(found_dict['H'])
        elif group_key == 'I':
            hour = int(found_dict['I'])
            ampm = found_dict.get('p', '').lower()
            if ampm in ('', _AM_PM[0]):
                if hour == 12:
                    hour = 0
            elif ampm == _AM_PM[1]:
                if hour != 12:
                    hour += 12
        elif group_key == 'M':
            minute = int(found_dict['M'])
        elif group_key == 'S':
            second = int(found_dict['S'])
        elif group_key == 'f':
            s = found_dict['f']
            s += "0" * (6 - len(s))
            fraction = int(s)
        elif group_key == 'A':
            weekday = _F_WEEKDAY.index(found_dict['A'].lower())
        elif group_key == 'a':
            weekday = _A_WEEKDAY.index(found_dict['a'].lower())
        elif group_key == 'w':
            weekday = int(found_dict['w'])
            if weekday == 0:
                weekday = 6
            else:
                weekday -= 1
        elif group_key == 'u':
            weekday = int(found_dict['u'])
            weekday -= 1
        elif group_key == 'j':
            julian = int(found_dict['j'])
        elif group_key in ('U', 'W'):
            week_of_year = int(found_dict[group_key])
            if group_key == 'U':
                week_of_year_start = 6
            else:
                week_of_year_start = 0
        elif group_key == 'V':
            iso_week = int(found_dict['V'])
        elif group_key == 'z':
            z = found_dict['z']
            if z == 'Z':
                gmtoff = 0
            else:
                if z[3] == ':':
                    z = z[:3] + z[4:]
                    if len(z) > 5:
                        if z[5] != ':':
                            raise ValueError(f"Inconsistent use of : in {found_dict['z']}")
                        z = z[:5] + z[6:]
                hours = int(z[1:3])
                minutes = int(z[3:5])
                seconds = int(z[5:7] or 0)
                gmtoff = (hours * 60 * 60) + (minutes * 60) + seconds
                gmtoff_remainder = z[8:]
                gmtoff_remainder_padding = "0" * (6 - len(gmtoff_remainder))
                gmtoff_fraction = int(gmtoff_remainder + gmtoff_remainder_padding)
                if z.startswith("-"):
                    gmtoff = -gmtoff
                    gmtoff_fraction = -gmtoff_fraction
        elif group_key == 'Z':
            found_zone = found_dict['Z'].lower()
            for value, tz_values in enumerate(_TIMEZONE):
                if found_zone in tz_values:
                    if (time.tzname[0] == time.tzname[1] and
                       time.daylight and found_zone not in ("utc", "gmt")):
                        break
                    else:
                        tz = value
                        break

    if iso_year is not None:
        if julian is not None:
            raise ValueError("Day of the year directive '%j' is not "
                             "compatible with ISO year directive '%G'. "
                             "Use '%Y' instead.")
        elif iso_week is None or weekday is None:
            raise ValueError("ISO year directive '%G' must be used with "
                             "the ISO week directive '%V' and a weekday "
                             "directive ('%A', '%a', '%w', or '%u').")
    elif iso_week is not None:
        if year is None or weekday is None:
            raise ValueError("ISO week directive '%V' must be used with "
                             "the ISO year directive '%G' and a weekday "
                             "directive ('%A', '%a', '%w', or '%u').")
        else:
            raise ValueError("ISO week directive '%V' is incompatible with "
                             "the year directive '%Y'. Use the ISO year '%G' "
                             "instead.")

    leap_year_fix = False
    if year is None:
        if month == 2 and day == 29:
            year = 1904
            leap_year_fix = True
        else:
            year = 1900

    if julian is None and weekday is not None:
        if week_of_year is not None:
            week_starts_Mon = True if week_of_year_start == 0 else False
            julian = _calc_julian_from_U_or_W(year, week_of_year, weekday,
                                              week_starts_Mon)
        elif iso_year is not None and iso_week is not None:
            datetime_result = datetime_date.fromisocalendar(iso_year, iso_week, weekday + 1)
            year = datetime_result.year
            month = datetime_result.month
            day = datetime_result.day
        if julian is not None and julian <= 0:
            year -= 1
            yday = 366 if (year % 4 == 0 and (year % 100 != 0 or year % 400 == 0)) else 365
            julian += yday

    if julian is None:
        julian = datetime_date(year, month, day).toordinal() - \
                  datetime_date(year, 1, 1).toordinal() + 1
    else:
        datetime_result = datetime_date.fromordinal(
                            (julian - 1) +
                            datetime_date(year, 1, 1).toordinal())
        year = datetime_result.year
        month = datetime_result.month
        day = datetime_result.day
    if weekday is None:
        weekday = datetime_date(year, month, day).weekday()
    tzname = found_dict.get("Z")

    if leap_year_fix:
        year = 1900

    return (year, month, day,
            hour, minute, second,
            weekday, julian, tz, tzname, gmtoff), fraction, gmtoff_fraction


def _strptime_datetime_date(cls, data_string, format="%a %b %d %Y"):
    tt, _, _ = _strptime(data_string, format)
    args = tt[:3]
    return cls(*args)


def _parse_tz(tzname, gmtoff, gmtoff_fraction):
    if not tzname:
        return _offset_zone(gmtoff * 1000000 + gmtoff_fraction)
    tzdelta = datetime_timedelta(seconds=gmtoff, microseconds=gmtoff_fraction)
    return datetime_timezone(tzdelta, tzname)


def _strptime_datetime_time(cls, data_string, format="%H:%M:%S"):
    tt, fraction, gmtoff_fraction = _strptime(data_string, format)
    tzname, gmtoff = tt[-2:]
    args = tt[3:6] + (fraction,)
    if gmtoff is None:
        return cls(*args)
    else:
        tz = _parse_tz(tzname, gmtoff, gmtoff_fraction)
        return cls(*args, tz)


def _strptime_datetime_datetime(cls, data_string, format="%a %b %d %H:%M:%S %Y"):
    r = _dt_strptime(data_string, format)
    tt, fraction, gmtoff_fraction = _strptime(data_string, format) if r is None else r
    tzname, gmtoff = tt[-2:]
    if r is not None and cls is datetime_datetime and tt[5] <= 59:
        # _mm.dt_strptime checked the date; the other fields are in range.
        tz = None if gmtoff is None else _parse_tz(tzname, gmtoff, gmtoff_fraction)
        return _new_datetime(cls, *tt[:6], fraction, tz)
    args = tt[:6] + (fraction,)
    if gmtoff is None:
        return cls(*args)
    else:
        tz = _parse_tz(tzname, gmtoff, gmtoff_fraction)
        return cls(*args, tz)
