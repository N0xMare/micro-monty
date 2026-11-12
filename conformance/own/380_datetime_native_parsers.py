# datetime's native parsers: fromisoformat as CPython's C module parses
# (strict ASCII digits, the bytes past the separator, hour 24, week dates,
# offsets out of range) and strptime's common directives, which must agree
# with the Python _strptime they stand in for.
import time as _t
from datetime import date, datetime, time, timedelta, timezone


def show(f, *a):
    try:
        r = f(*a)
    except (ValueError, TypeError, OverflowError) as e:
        return f'{type(e).__name__}: {e}'
    return tuple(r) if isinstance(r, _t.struct_time) else repr(r)


ISO = ['2026-03-09T10:30:00+00:00', '2026-03-09T10:30:00Z', '2026-03-09 10:30', '2026-03-09T10:30:00.123+05:30',
       '2026-03-09T10:30:00,5-08:00', '2026-03-09', '2026-03-09T', '2026-03-09 ', '2024-02-29T23:59:59.9999999',
       '2026-03-09T24:00', '2026-12-31T24:00', '9999-12-31T24:00', '2026-02-29T24:00', '2026-02-28T24:00:00.000001',
       '2026-00-00T24:00', '2026-W10-1T10:00', '2026W103', '2026-W53-1', '2020-W53-7T10', '9999-W52-6', '2026-W54-1',
       '20260309T103000', '20260309xx', '2026-03-09T10:30+05:75', '2026-03-09T10:30-23:59:59.999999',
       '2026-03-09T10:30+24:00', '2026-03-09T10:30+99:99', '2026-03-09T10:3', '2026-03-09T10:30:00.',
       '10:30:00.+05:00', '2026-03-09T10:30Z05', '0000-01-01', '2026-13-01', '2026-02-30', '2026-03-09T1:30',
       '2026-03-09T10:30:00+0530', '2026-03-09T10:30:00+05', '2026-03-09T10:30:00+053', '2026-03-09x10',
       '2026-03-09\u00e9101500', '2026-03-09\u4e00101500', '\uff12\uff10\uff12\uff16-01-01', '2026-03-09T10:30:00.1234567891',
       '2026-03-09T10:30:00.123a', '10:30\x00', '2026-03-09T10:30\x00x', '10300012', '25:00', '24:00', '24:00:01',
       '10:61', 'T10:30+24:00', 'T', '', '2026-03', '2026-3-9', '1030', '10:30:', '10.5', '10:30.5', '2026-03-09T10:30:00:00']
for s in ISO:
    print(repr(s), show(datetime.fromisoformat, s), show(date.fromisoformat, s), show(time.fromisoformat, s))
for f in (datetime.fromisoformat, date.fromisoformat, time.fromisoformat):
    print(show(f, 1), show(f, b'2026-03-09'))


class D(datetime):
    pass


class T(time):
    pass


print(repr(D.fromisoformat('2026-03-09T10:30+01:00')), type(D.fromisoformat('2026-03-09')).__name__,
      repr(T.fromisoformat('10:30Z')))
a = datetime.fromisoformat('2026-03-09T10:30:00.5-08:00')
print(a.utcoffset(), a.tzinfo, a.tzinfo == timezone(timedelta(hours=-8)), a.tzinfo is datetime.fromisoformat('2026-01-01T00:00Z').tzinfo,
      datetime.fromisoformat('2026-01-01T00:00Z').tzinfo is timezone.utc, hash(a) == hash(datetime(2026, 3, 9, 18, 30, 0, 500000, timezone.utc)))
print(a == datetime(2026, 3, 9, 10, 30, 0, 500000, timezone(timedelta(hours=-8))), a.replace(tzinfo=None), a.astimezone(timezone.utc))

STRP = [('2026-03-09 10:30:00', '%Y-%m-%d %H:%M:%S'), ('Mon Mar  9 10:30:00 2026', '%a %b %d %H:%M:%S %Y'),
('09/03/26 10:30PM', '%d/%m/%y %I:%M%p'), ('12:00 am', '%I:%M %p'),
        ('12:00 PM', '%I:%M %p'), ('2026-03-09T10:30:00+05:30', '%Y-%m-%dT%H:%M:%S%z'), ('2026-03-09T10:30:00Z', '%Y-%m-%dT%H:%M:%S%z'),
        ('2026-03-09T10:30:00-0800', '%Y-%m-%dT%H:%M:%S%z'), ('2026-03-09T10:30:00.5+05:30:15.25', '%Y-%m-%dT%H:%M:%S.%f%z'),
        ('2026-03-09 +05:3000', '%Y-%m-%d %z'), ('2026-03-09 +0530:00', '%Y-%m-%d %z'), ('2026-03-09 +25:00', '%Y-%m-%d %z'),
        ('2026-03-09 z', '%Y-%m-%d %z'), ('2026 068', '%Y %j'), ('2026 366', '%Y %j'), ('2024 366', '%Y %j'), ('9999 366', '%Y %j'),
        ('2026-02-30', '%Y-%m-%d'), ('2026-03-09 extra', '%Y-%m-%d'), ('2026-3-9', '%Y-%m-%d'), ('March 9, 2026', '%B %d, %Y'),
        ('MARCH 9, 2026', '%B %d, %Y'), ('t2026', 'T%Y'), ('2026-03-09T10:30:60', '%Y-%m-%dT%H:%M:%S'), ('2026-03-09T10:30:61', '%Y-%m-%dT%H:%M:%S'),
        ('10:30', '%H:%M'), ('2026-03-09\t \n10', '%Y-%m-%d %H'), ('2026-03-0910', '%Y-%m-%d%H'), ('20260309', '%Y%m%d'),
        ('2026111', '%Y%m%d'), ('2026 1 2', '%Y %m %d'), ('Tue 2026-03-09', '%a %Y-%m-%d'), ('Tuesday 2026 068', '%A %Y %j'),
        ('2026-03-09 %', '%Y-%m-%d %%'), ('2026-03-09', '%Y-%m-%d %'), ('0000', '%Y'), ('2026 69', '%Y %y'), ('68', '%y'), ('69', '%y'),
        ('2026-03-09 1234567', '%Y-%m-%d %f'), ('2026-03-09 7', '%Y-%m-%d %f'), ('2026-03-09', '%Y-%m-%d%%'), ("2026's", "%Y's"),
        ('2026-03-09 10', '%Y-%m-%d %-H'), ('2026-03-09', '%Y-%m-%e'), ('2026-03-09 UTC', '%Y-%m-%d %Z')]
for s, f in STRP:
    print(repr(s), repr(f), show(datetime.strptime, s, f), show(_t.strptime, s, f), show(date.strptime, s, f),
          show(time.strptime, s, f))
print(show(_t.strptime, 'Mon Mar  9 10:30:00 2026'))
print(show(datetime.strptime, 1, '%Y'), show(datetime.strptime, '2026', 1), show(_t.strptime, '2026', 1))
print(repr(D.strptime('2026-03-09 +0100', '%Y-%m-%d %z')), datetime.strptime('2026-03-09 -0130', '%Y-%m-%d %z').tzinfo)
