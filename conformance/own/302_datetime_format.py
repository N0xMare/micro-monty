# datetime: formatting, parsing and arithmetic.
from datetime import date, datetime, time, timedelta, timezone

d = datetime(2024, 2, 29, 13, 5, 9, 123456)
print(d.isoformat(), d.isoformat(' ', 'minutes'), d.isoformat(timespec='milliseconds'), str(d), repr(d))
print(d.strftime('%Y-%m-%d %H:%M:%S.%f %a %A %b %B %j %U %W %w %y %p %I %%'), d.strftime('%c|%x|%X'))
print(d.date(), d.time(), d.weekday(), d.isoweekday(), d.isocalendar(), d.toordinal(), d.timetuple()[:9])
print(datetime.fromisoformat('2024-02-29T13:05:09.123+05:30'), datetime.fromisoformat('20240229T130509Z'), date.fromisoformat('2024-W09-4'))
print(datetime.strptime('29/02/2024 1:05 PM', '%d/%m/%Y %I:%M %p'), datetime.strptime('2024-060', '%Y-%j').date())
tz = timezone(timedelta(hours=-3, minutes=-30), 'NST')
a = datetime(2024, 1, 1, 12, tzinfo=timezone.utc)
print(a.astimezone(tz), a.astimezone(tz).utcoffset(), tz, repr(tz), a.tzname(), timezone(timedelta(hours=2)))
print(a - datetime(2023, 12, 31, 23, 59, tzinfo=tz), a + timedelta(days=1, seconds=-1), a.timestamp(), datetime.fromtimestamp(0, timezone.utc))
t = timedelta(days=-1, hours=3, microseconds=7)
print(t, repr(t), t.total_seconds(), abs(t), t * 2.5, t // 3, t / timedelta(hours=1), divmod(timedelta(hours=25), timedelta(days=1)))
print(timedelta(weeks=1) > timedelta(days=6), date(2024, 3, 1) - date(2024, 2, 1), date(2024, 1, 31).replace(month=2, day=29))
print(time(23, 59, 59, 999999).isoformat(), time(1, 2, tzinfo=timezone.utc).isoformat(), time.fromisoformat('04:23:01.000384'), time(12).strftime('%I %p'))
print(date.max, date.min, datetime.max, timedelta.max, timedelta.resolution, date(2024, 5, 17).ctime())
for bad in [lambda: date(2023, 2, 29), lambda: datetime(2024, 1, 1, 24), lambda: datetime.fromisoformat('2024-13-01'),
            lambda: timedelta(days=10**10), lambda: datetime.strptime('x', '%Y'), lambda: date.today() < 1]:
    try:
        bad()
    except (ValueError, OverflowError, TypeError) as e:
        print(type(e).__name__, e)
print(hash(datetime(2024, 1, 1)) == hash(datetime(2024, 1, 1)), {date(2024, 1, 1): 1}[date(2024, 1, 1)])
print(sorted([date(2024, 3, 1), date(2023, 1, 1)]), max(time(1), time(0, 30)), datetime.combine(date(2024, 1, 2), time(3, 4)))
