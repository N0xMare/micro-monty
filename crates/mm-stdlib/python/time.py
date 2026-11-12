# time: the clocks read the host's wall clock (`_mm.clock`, which the
# embedder sets; docs/spec.md §10), and `sleep` does not block: it advances
# the clocks this module reports, unless the host registered a `time.sleep`
# function to call instead. The local zone is the host's (`_mm.local_zone`:
# TZif data, read once here), else UTC. Conversions follow
# CPython's Modules/timemodule.c (checks and messages) with C-locale names.

import _mm
from operator import index as _index

_MISSING = object()

_STRUCT_TM_ITEMS = 11

timezone = 0
altzone = 0
daylight = 0
tzname = ('UTC', 'UTC')

CLOCK_REALTIME = 0
CLOCK_MONOTONIC = 1
CLOCK_PROCESS_CPUTIME_ID = 2
CLOCK_THREAD_CPUTIME_ID = 3

# Seconds `sleep` has advanced the clocks, the last monotonic reading, and
# the monotonic reading at import (where the process clocks start).
_slept = 0.0
_last = 0.0
_DAYS = ('Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun')
_FULL_DAYS = ('Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday')
_MONTHS = ('Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec')
_FULL_MONTHS = ('January', 'February', 'March', 'April', 'May', 'June', 'July', 'August',
                'September', 'October', 'November', 'December')
_NAMES = ('tm_year', 'tm_mon', 'tm_mday', 'tm_hour', 'tm_min', 'tm_sec', 'tm_wday', 'tm_yday',
          'tm_isdst')


def _no_args(name, args, kwargs):
    if kwargs:
        raise TypeError(f'time.{name}() takes no keyword arguments')
    if args:
        raise TypeError(f'time.{name}() takes no arguments ({len(args)} given)')


def _now():
    return _mm.clock() + _slept


def _mono():
    global _last
    t = _now()
    if t < _last:
        t = _last
    _last = t
    return t


def time(*args, **kwargs):
    _no_args('time', args, kwargs)
    return _now()


def time_ns(*args, **kwargs):
    _no_args('time_ns', args, kwargs)
    return int(_now() * 1e9)


def monotonic(*args, **kwargs):
    _no_args('monotonic', args, kwargs)
    return _mono()


def monotonic_ns(*args, **kwargs):
    _no_args('monotonic_ns', args, kwargs)
    return int(_mono() * 1e9)


def perf_counter(*args, **kwargs):
    _no_args('perf_counter', args, kwargs)
    return _mono()


def perf_counter_ns(*args, **kwargs):
    _no_args('perf_counter_ns', args, kwargs)
    return int(_mono() * 1e9)


try:
    _START = _mono()
except RuntimeError:
    _START = 0.0


def _process():
    try:
        return max(_mono() - _START, 0.0)
    except RuntimeError:
        return 0.0


def process_time(*args, **kwargs):
    _no_args('process_time', args, kwargs)
    return _process()


def process_time_ns(*args, **kwargs):
    _no_args('process_time_ns', args, kwargs)
    return int(_process() * 1e9)


def thread_time(*args, **kwargs):
    _no_args('thread_time', args, kwargs)
    return _process()


def thread_time_ns(*args, **kwargs):
    _no_args('thread_time_ns', args, kwargs)
    return int(_process() * 1e9)


def _seconds(secs):
    # `_PyTime_FromSecondsObject`: a float, or anything with `__index__`.
    if isinstance(secs, float):
        if secs != secs:
            raise ValueError('Invalid value NaN (not a number)')
        if not -9223372036.854775 <= secs <= 9223372036.854775:
            raise OverflowError('timestamp out of range for C PyTime_t')
        return secs
    if not isinstance(secs, int) and not hasattr(type(secs), '__index__'):
        raise TypeError(f"'{type(secs).__name__}' object cannot be interpreted as an integer "
                        'or float')
    n = _index(secs)
    if not -9223372036 <= n <= 9223372036:
        raise OverflowError('timestamp out of range for C PyTime_t')
    return n


def sleep(*args, **kwargs):
    global _slept
    if kwargs:
        raise TypeError('time.sleep() takes no keyword arguments')
    if len(args) != 1:
        raise TypeError(f'time.sleep() takes exactly one argument ({len(args)} given)')
    secs = _seconds(args[0])
    if secs < 0:
        raise ValueError('sleep length must be non-negative')
    host = _mm.os_function('time.sleep')
    if host is not None:
        host(secs)
        return None
    _slept += secs
    return None


class struct_time(tuple):
    __mm_tpname__ = 'time.struct_time'
    n_sequence_fields = 9
    n_fields = 11
    n_unnamed_fields = 0

    def __new__(cls, *args, **kwargs):
        if kwargs or len(args) != 1:
            raise TypeError('struct_time() takes exactly 1 argument')
        seq = tuple(args[0])
        n = len(seq)
        if n < 9:
            raise TypeError(f'time.struct_time() takes an at least 9-sequence ({n}-sequence given)')
        if n > 11:
            raise TypeError(f'time.struct_time() takes an at most 11-sequence ({n}-sequence given)')
        self = tuple.__new__(cls, seq[:9])
        self._extra = seq[9:] + (None,) * (11 - n)
        return self

    tm_year = property(lambda self: self[0])
    tm_mon = property(lambda self: self[1])
    tm_mday = property(lambda self: self[2])
    tm_hour = property(lambda self: self[3])
    tm_min = property(lambda self: self[4])
    tm_sec = property(lambda self: self[5])
    tm_wday = property(lambda self: self[6])
    tm_yday = property(lambda self: self[7])
    tm_isdst = property(lambda self: self[8])
    tm_zone = property(lambda self: self._extra[0])
    tm_gmtoff = property(lambda self: self._extra[1])

    def __repr__(self):
        fields = ', '.join([f'{n}={v!r}' for n, v in zip(_NAMES, self)])
        return f'time.struct_time({fields})'

    def __reduce__(self):
        return (struct_time, (tuple(self) + self._extra,))


struct_time.__module__ = 'time'


def _days_from_civil(y, m, d):
    y -= m <= 2
    era = (y if y >= 0 else y - 399) // 400
    yoe = y - era * 400
    doy = (153 * (m + (-3 if m > 2 else 9)) + 2) // 5 + d - 1
    doe = yoe * 365 + yoe // 4 - yoe // 100 + doy
    return era * 146097 + doe - 719468


def _is_leap(y):
    return y % 4 == 0 and (y % 100 != 0 or y % 400 == 0)


def _tm(t, offset, zone, isdst=0):
    # The struct_time of POSIX time `t` (an int) seen at UTC offset `offset`.
    t += offset
    days, secs = divmod(t, 86400)
    z = days + 719468
    era = (z if z >= 0 else z - 146096) // 146097
    doe = z - era * 146097
    yoe = (doe - doe // 1460 + doe // 36524 - doe // 146096) // 365
    y = yoe + era * 400
    doy = doe - (365 * yoe + yoe // 4 - yoe // 100)
    mp = (5 * doy + 2) // 153
    d = doy - (153 * mp + 2) // 5 + 1
    m = mp + 3 if mp < 10 else mp - 9
    y += m <= 2
    yday = days - _days_from_civil(y, 1, 1) + 1
    return struct_time((y, m, d, secs // 3600, secs // 60 % 60, secs % 60, (days + 3) % 7,
                        yday, isdst, zone, offset))


# ------------------------------------------------------------ local zone
# The host's TZif data (RFC 8536): transition instants, the local time type
# from each, the types (UTC offset, is DST, abbreviation) and the footer's
# POSIX TZ rule for instants after the last transition.

_trans = []
_tidx = []
_types = None
_rule = None


def _load_zone(b):
    global _trans, _tidx, _types, _rule
    if b[:4] != b'TZif':
        return

    def count(pos):
        return [int.from_bytes(b[pos + 20 + 4 * k:pos + 24 + 4 * k], 'big') for k in range(6)]

    isut, isstd, leap, timecnt, typecnt, charcnt = count(0)
    pos = 44
    size = 4
    if b[4] >= 0x32:
        # Version 2+: skip the 32-bit block for the 64-bit one.
        pos += timecnt * 5 + typecnt * 6 + charcnt + leap * 8 + isstd + isut
        isut, isstd, leap, timecnt, typecnt, charcnt = count(pos)
        pos += 44
        size = 8
    trans = [int.from_bytes(b[pos + size * k:pos + size * k + size], 'big', signed=True)
             for k in range(timecnt)]
    pos += size * timecnt
    tidx = list(b[pos:pos + timecnt])
    pos += timecnt
    raw = []
    for k in range(typecnt):
        raw.append((int.from_bytes(b[pos:pos + 4], 'big', signed=True), b[pos + 4], b[pos + 5]))
        pos += 6
    chars = b[pos:pos + charcnt]
    pos += charcnt + leap * (size + 4) + isstd + isut
    types = []
    for off, dst, ai in raw:
        end = chars.find(b'\0', ai)
        types.append((off, dst, chars[ai:end if end >= 0 else len(chars)].decode('ascii')))
    if size == 8 and b[pos:pos + 1] == b'\n':
        end = b.find(b'\n', pos + 1)
        if end > pos + 1:
            _rule = _parse_rule(b[pos + 1:end].decode('ascii'))
    if not types:
        types = [(0, 0, 'UTC')]
    _trans, _tidx, _types = trans, tidx, types


def _rule_name(s, i):
    if s[i:i + 1] == '<':
        j = s.index('>', i)
        return s[i + 1:j], j + 1
    j = i
    while j < len(s) and s[j].isalpha():
        j += 1
    return s[i:j], j


def _rule_secs(s, i):
    # [+-]hh[:mm[:ss]] as seconds.
    sign = 1
    if s[i:i + 1] in ('+', '-'):
        sign = -1 if s[i] == '-' else 1
        i += 1
    parts = [0, 0, 0]
    k = 0
    while True:
        j = i
        while j < len(s) and s[j].isdigit():
            j += 1
        parts[k] = int(s[i:j] or '0')
        i = j
        if k < 2 and s[i:i + 1] == ':':
            i += 1
            k += 1
        else:
            break
    return sign * (parts[0] * 3600 + parts[1] * 60 + parts[2]), i


def _rule_date(s, i):
    # Mm.w.d, Jn or n, then an optional /time (default 02:00).
    if s[i] == 'M':
        j = s.index('.', i)
        k = s.index('.', j + 1)
        e = k + 1
        while e < len(s) and s[e].isdigit():
            e += 1
        date = ('M', int(s[i + 1:j]), int(s[j + 1:k]), int(s[k + 1:e]))
    else:
        kind = 'J' if s[i] == 'J' else 'n'
        if kind == 'J':
            i += 1
        e = i
        while e < len(s) and s[e].isdigit():
            e += 1
        date = (kind, int(s[i:e]), 0, 0)
    secs = 7200
    if s[e:e + 1] == '/':
        secs, e = _rule_secs(s, e + 1)
    return (date, secs), e


def _parse_rule(s):
    try:
        std, i = _rule_name(s, 0)
        off, i = _rule_secs(s, i)
        if i >= len(s):
            return (std, -off, None, None, None, None)
        dst, i = _rule_name(s, i)
        dstoff = -off + 3600
        if i < len(s) and s[i] != ',':
            o, i = _rule_secs(s, i)
            dstoff = -o
        if i < len(s) and s[i] == ',':
            start, i = _rule_date(s, i + 1)
            end, i = _rule_date(s, i + 1)
        else:
            start, end = (('M', 3, 2, 0), 7200), (('M', 11, 1, 0), 7200)
        return (std, -off, dst, dstoff, start, end)
    except (ValueError, IndexError):
        return None


def _rule_day(y, date):
    # Days since the epoch of the rule date in year `y`.
    kind, a, w, d = date
    jan1 = _days_from_civil(y, 1, 1)
    if kind == 'J':
        return jan1 + a - 1 + (a >= 60 and _is_leap(y))
    if kind == 'n':
        return jan1 + a
    first = _days_from_civil(y, a, 1)
    mdays = _days_from_civil(y + (a == 12), a % 12 + 1, 1) - first
    day = (d - (first + 4)) % 7 + (w - 1) * 7
    while day >= mdays:
        day -= 7
    return first + day


def _rule_local(t):
    std, stdoff, dst, dstoff, start, end = _rule
    if dst is None:
        return (stdoff, 0, std)
    y = _tm(t, stdoff, '')[0]
    # glibc computes the changes of any year before 1970 as 1970's.
    y = max(y, 1970)
    a = _rule_day(y, start[0]) * 86400 + start[1] - stdoff
    b = _rule_day(y, end[0]) * 86400 + end[1] - dstoff
    if a < b:
        isdst = a <= t < b
    else:
        isdst = not (b <= t < a)
    return (dstoff, 1, dst) if isdst else (stdoff, 0, std)


def _local(t):
    # (UTC offset, is DST, abbreviation) at POSIX time `t`.
    if _types is None:
        return (0, 0, 'UTC')
    if _trans and t < _trans[0]:
        return _types[0]
    if not _trans or t >= _trans[-1]:
        if _rule is not None:
            return _rule_local(t)
        return _types[_tidx[-1]] if _trans else _types[0]
    lo, hi = 0, len(_trans)
    while lo < hi:
        mid = (lo + hi) // 2
        if _trans[mid] <= t:
            lo = mid + 1
        else:
            hi = mid
    return _types[_tidx[lo - 1]]


def _local_tm(t):
    off, dst, abbr = _local(t)
    return _tm(t, off, abbr, 1 if dst else 0)


def _init_zone():
    # `init_timezone` on glibc: the zones of January and July of this year.
    global timezone, altzone, daylight, tzname
    _load_zone(_mm.local_zone() or b'')
    if _types is None:
        return
    try:
        now = int(_now())
    except RuntimeError:
        now = 0
    year = (365 * 24 + 6) * 3600
    t = now // year * year
    jan = _local(t)
    jul = _local(t + year // 2)
    if -jan[0] < -jul[0]:
        timezone, altzone, tzname = -jul[0], -jan[0], (jul[2], jan[2])
    else:
        timezone, altzone, tzname = -jan[0], -jul[0], (jan[2], jul[2])
    daylight = int(jan[0] != jul[0])


def _timestamp(fname, args, kwargs):
    if kwargs:
        raise TypeError(f'{fname}() takes no keyword arguments')
    if len(args) > 1:
        raise TypeError(f'{fname}() takes at most 1 argument ({len(args)} given)')
    if not args or args[0] is None:
        return int(_now() // 1)
    t = args[0]
    if isinstance(t, float):
        if t != t:
            raise ValueError('Invalid value NaN (not a number)')
        if not -9.2e18 < t < 9.2e18:
            raise OverflowError('timestamp out of range for platform time_t')
        return int(t // 1)
    t = _index(t)
    if not -9223372036854775808 <= t <= 9223372036854775807:
        raise OverflowError('timestamp out of range for platform time_t')
    return t


def _check_year(t):
    # glibc fails for a year beyond a C int.
    if not -67768040609740800 <= t <= 67767976233532799:
        raise OverflowError('timestamp out of range for platform time_t')


def gmtime(*args, **kwargs):
    t = _timestamp('gmtime', args, kwargs)
    _check_year(t)
    # glibc's gmtime names its zone GMT; the local zone here is UTC.
    return _tm(t, 0, 'GMT')


def localtime(*args, **kwargs):
    t = _timestamp('localtime', args, kwargs)
    _check_year(t)
    return _local_tm(t)


def _tuple(fname, t):
    # `gettmarg`: a 9-item tuple or a struct_time, as C struct tm fields.
    if not isinstance(t, tuple):
        raise TypeError('Tuple or struct_time argument required')
    if len(t) != 9 and not isinstance(t, struct_time):
        raise TypeError(f'{fname}(): illegal time tuple argument')
    v = []
    for x in t[:9]:
        v.append(_index(x))
    # C fields: month 0-based, Sunday-first weekday, 0-based day of year.
    v[1] -= 1
    v[6] = (v[6] + 1) % 7 if v[6] >= -1 else -1
    v[7] -= 1
    return v


def _checktm(v):
    if not 0 <= v[1] <= 11:
        raise ValueError('month out of range')
    if not 1 <= v[2] <= 31:
        raise ValueError('day of month out of range')
    if not 0 <= v[3] <= 23:
        raise ValueError('hour out of range')
    if not 0 <= v[4] <= 59:
        raise ValueError('minute out of range')
    if not 0 <= v[5] <= 61:
        raise ValueError('seconds out of range')
    if v[6] < 0:
        raise ValueError('day of week out of range')
    if not 0 <= v[7] <= 365:
        raise ValueError('day of year out of range')


def _asctime(v):
    return (f'{_DAYS[(v[6] + 6) % 7]} {_MONTHS[v[1]]} {v[2]:2d} '
            f'{v[3]:02d}:{v[4]:02d}:{v[5]:02d} {v[0]}')


def asctime(*args, **kwargs):
    if kwargs:
        raise TypeError('asctime() takes no keyword arguments')
    if len(args) > 1:
        raise TypeError(f'asctime expected at most 1 argument, got {len(args)}')
    v = _tuple('asctime', args[0] if args else localtime())
    _checktm(v)
    return _asctime(v)


def ctime(*args, **kwargs):
    t = _timestamp('ctime', args, kwargs)
    _check_year(t)
    return _asctime(_tuple('ctime', _local_tm(t)))


def mktime(*args, **kwargs):
    if kwargs:
        raise TypeError('time.mktime() takes no keyword arguments')
    if len(args) != 1:
        raise TypeError(f'time.mktime() takes exactly one argument ({len(args)} given)')
    v = _tuple('mktime', args[0])
    y, mon = v[0] + v[1] // 12, v[1] % 12
    days = _days_from_civil(y, mon + 1, 1) + v[2] - 1
    t = days * 86400 + v[3] * 3600 + v[4] * 60 + v[5]
    if _types is None:
        return float(t)
    # glibc's mktime: the offsets in effect around the wall time; the one
    # that reproduces it (preferring `tm_isdst`'s kind), else, in a gap,
    # the offset before it.
    found = []
    for probe in (t - 86400, t, t + 86400):
        off = _local(probe - _local(probe)[0])[0]
        loc = _local(t - off)
        if loc[0] == off and (off, loc[1]) not in found:
            found.append((off, loc[1]))
    if not found:
        return float(t - _local(t - 86400)[0])
    for off, dst in found:
        if v[8] < 0 or (dst > 0) == (v[8] > 0):
            return float(t - off)
    return float(t - found[0][0])


def _iso(y, yday, wday):
    # ISO year and week of a day: `yday` 0-based, `wday` Monday=0.
    week = (yday - wday + 10) // 7
    if week < 1:
        y -= 1
        return _iso(y, yday + (366 if _is_leap(y) else 365), wday)
    if week == 53 and (yday - wday) >= (366 if _is_leap(y) else 365) - 3:
        return y + 1, 1
    return y, week


def _strftime(fmt, v, zone, gmtoff):
    # C-locale strftime over C struct tm fields `v`.
    out = []
    i = 0
    n = len(fmt)
    while i < n:
        c = fmt[i]
        if c != '%' or i + 1 >= n:
            out.append(c)
            i += 1
            continue
        d = fmt[i + 1]
        i += 2
        y, mon, mday, hh, mm, ss, wday, yday = v[:8]
        mwday = (wday + 6) % 7
        if d == 'Y':
            out.append(str(y))
        elif d == 'm':
            out.append(f'{mon + 1:02d}')
        elif d == 'd':
            out.append(f'{mday:02d}')
        elif d == 'e':
            out.append(f'{mday:2d}')
        elif d == 'H':
            out.append(f'{hh:02d}')
        elif d == 'I':
            out.append(f'{(hh + 11) % 12 + 1:02d}')
        elif d == 'M':
            out.append(f'{mm:02d}')
        elif d == 'S':
            out.append(f'{ss:02d}')
        elif d == 'y':
            out.append(f'{y % 100:02d}')
        elif d == 'C':
            out.append(f'{y // 100:02d}')
        elif d == 'j':
            out.append(f'{yday + 1:03d}')
        elif d == 'a':
            out.append(_DAYS[mwday])
        elif d == 'A':
            out.append(_FULL_DAYS[mwday])
        elif d == 'b' or d == 'h':
            out.append(_MONTHS[mon])
        elif d == 'B':
            out.append(_FULL_MONTHS[mon])
        elif d == 'p':
            out.append('AM' if hh < 12 else 'PM')
        elif d == 'w':
            out.append(str(wday))
        elif d == 'u':
            out.append(str(mwday + 1))
        elif d == 'U':
            out.append(f'{(yday + 7 - wday) // 7:02d}')
        elif d == 'W':
            out.append(f'{(yday + 7 - mwday) // 7:02d}')
        elif d == 'G' or d == 'g' or d == 'V':
            iy, iw = _iso(y, yday, mwday)
            out.append(str(iy) if d == 'G' else f'{iy % 100:02d}' if d == 'g' else f'{iw:02d}')
        elif d == 'z':
            if gmtoff is None:
                out.append('')
            else:
                sign = '-' if gmtoff < 0 else '+'
                a = abs(gmtoff) // 60
                out.append(f'{sign}{a // 60:02d}{a % 60:02d}')
        elif d == 'Z':
            out.append(zone or '')
        elif d == 'c':
            out.append(_strftime('%a %b %e %H:%M:%S %Y', v, zone, gmtoff))
        elif d == 'x' or d == 'D':
            out.append(_strftime('%m/%d/%y', v, zone, gmtoff))
        elif d == 'X' or d == 'T':
            out.append(_strftime('%H:%M:%S', v, zone, gmtoff))
        elif d == 'F':
            out.append(_strftime('%Y-%m-%d', v, zone, gmtoff))
        elif d == 'R':
            out.append(_strftime('%H:%M', v, zone, gmtoff))
        elif d == 'r':
            out.append(_strftime('%I:%M:%S %p', v, zone, gmtoff))
        elif d == 'n':
            out.append('\n')
        elif d == 't':
            out.append('\t')
        elif d == '%':
            out.append('%')
        else:
            out.append('%' + d)
    return ''.join(out)


def strftime(*args, **kwargs):
    if kwargs:
        raise TypeError('strftime() takes no keyword arguments')
    if not args:
        raise TypeError('strftime() takes at least 1 argument (0 given)')
    if len(args) > 2:
        raise TypeError(f'strftime() takes at most 2 arguments ({len(args)} given)')
    fmt = args[0]
    if not isinstance(fmt, str):
        raise TypeError(f'strftime() argument 1 must be str, not {type(fmt).__name__}')
    t = args[1] if len(args) > 1 else localtime()
    v = _tuple('strftime', t)
    if v[1] == -1:
        v[1] = 0
    if v[2] == 0:
        v[2] = 1
    if v[7] == -1:
        v[7] = 0
    _checktm(v)
    if isinstance(t, struct_time) and t.tm_zone is not None:
        zone, gmtoff = t.tm_zone, t.tm_gmtoff
    else:
        zone, gmtoff = tzname[v[8] > 0], -(altzone if v[8] > 0 else timezone)
    return _strftime(fmt, v, zone, gmtoff)


def _strptime_time(data_string, format='%a %b %d %H:%M:%S %Y'):
    from datetime._strptime import _strptime
    tt = _strptime(data_string, format)[0]
    return struct_time(tt[:_STRUCT_TM_ITEMS])


strptime = _strptime_time


class _ClockInfo:
    # types.SimpleNamespace's look, for get_clock_info().
    def __init__(self, implementation, monotonic, adjustable, resolution):
        self.implementation = implementation
        self.monotonic = monotonic
        self.adjustable = adjustable
        self.resolution = resolution

    def __repr__(self):
        return (f'namespace(implementation={self.implementation!r}, '
                f'monotonic={self.monotonic!r}, adjustable={self.adjustable!r}, '
                f'resolution={self.resolution!r})')


def get_clock_info(name):
    # Every clock is the host's (time() its wall clock; the others are
    # derived from it and the virtual sleeps).
    if name == 'time':
        return _ClockInfo('host wall clock', False, True, 1e-09)
    if name in ('monotonic', 'perf_counter', 'process_time', 'thread_time'):
        return _ClockInfo('host wall clock + virtual sleeps', True, False, 1e-09)
    raise ValueError('unknown clock')


_init_zone()
