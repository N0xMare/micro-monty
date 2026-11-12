# zoneinfo over the host's zone source (mm run serves /usr/share/zoneinfo):
# DST gaps and folds, the POSIX footer past the last transition, LMT, the
# cache, key validation, no_cache; io.BytesIO's relative seeks.
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError
import zoneinfo
from datetime import datetime, timedelta, timezone
paris = ZoneInfo("Europe/Paris")
ny = ZoneInfo("America/New_York")
print(repr(paris), str(ny), paris.key, ZoneInfo("Europe/Paris") is paris)
for dt in [datetime(2026, 3, 29, 0, 30), datetime(2026, 3, 29, 2, 30), datetime(2026, 10, 25, 2, 30),
           datetime(2026, 10, 25, 2, 30, fold=1), datetime(2099, 7, 1, 12), datetime(1900, 1, 1)]:
    d = dt.replace(tzinfo=paris)
    print(d.isoformat(), d.tzname(), d.utcoffset(), d.dst())
u = datetime(2026, 3, 15, 12, tzinfo=timezone.utc)
print(u.astimezone(ny), u.astimezone(ZoneInfo("Asia/Tokyo")), u.astimezone(ZoneInfo("Asia/Kolkata")).strftime("%H:%M %Z"))
print(datetime(2026, 11, 1, 1, 30, tzinfo=ny).tzname(), datetime(2026, 11, 1, 1, 30, fold=1, tzinfo=ny).tzname())
print((datetime(2026, 3, 8, 12, tzinfo=ny) - datetime(2026, 3, 7, 12, tzinfo=ny)))
print(datetime(2026, 3, 8, 12, tzinfo=ny).timestamp() - datetime(2026, 3, 7, 12, tzinfo=ny).timestamp())
print(ZoneInfo("UTC"), datetime(2026, 1, 1, tzinfo=ZoneInfo("UTC")).isoformat())
for bad in ["Mars/Olympus_Mons", "../etc/passwd", "/etc/localtime", "Europe//Paris", "Europe/Paris/"]:
    try:
        ZoneInfo(bad)
    except ZoneInfoNotFoundError as e:
        print('not found', e)
    except ValueError as e:
        print('ValueError', e)
print(issubclass(ZoneInfoNotFoundError, KeyError), zoneinfo.TZPATH == zoneinfo.TZPATH)
n = ZoneInfo.no_cache("Europe/Paris")
print(n is paris, n.key, n == paris)
import io
b = io.BytesIO(b'0123456789')
print(b.seek(3, 1), b.seek(-2, 1), b.seek(-100, 1), b.seek(-3, 2), b.seek(5, 2), b.read(1))
for args in [(-1,), (0, 3)]:
    try:
        b.seek(*args)
    except ValueError as e:
        print(e)
