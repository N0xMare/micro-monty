# Randomized differential case for aware datetime paths: run by datetime_aware.py
# on CPython and mm with SEED replaced; every output line must match.
import random, sys
from datetime import datetime, timedelta, timezone, tzinfo, date, time
from zoneinfo import ZoneInfo

class Fixed(tzinfo):
    def __init__(self, m): self.m = m
    def utcoffset(self, dt): return timedelta(minutes=self.m)
    def dst(self, dt): return timedelta(0)
    def tzname(self, dt): return f"F{self.m}"
    def __repr__(self): return f"Fixed({self.m})"

class FoldTz(tzinfo):
    # -5h, or -4h when fold is set: offsets depend on fold.
    def utcoffset(self, dt): return timedelta(hours=-4 if dt is not None and dt.fold else -5)
    def dst(self, dt): return timedelta(0)
    def __repr__(self): return "FoldTz()"

class NoneTz(tzinfo):
    def utcoffset(self, dt): return None
    def dst(self, dt): return None
    def __repr__(self): return "NoneTz()"

class BadTz(tzinfo):
    def __init__(self, v): self.v = v
    def utcoffset(self, dt): return self.v
    def dst(self, dt): return timedelta(0)
    def __repr__(self): return f"BadTz({self.v!r})"

class RaiseTz(tzinfo):
    def utcoffset(self, dt): raise KeyError("boom")
    def __repr__(self): return "RaiseTz()"

class Sub(timedelta):
    pass

class MyDT(datetime):
    pass

SEED = 1
rng = random.Random(SEED)
ZONES = [ZoneInfo(k) for k in ("America/New_York", "Europe/London", "Australia/Lord_Howe", "Asia/Kolkata", "UTC")]
TZS = ([None] * 4 + [timezone.utc, timezone(timedelta(hours=-8)), timezone(timedelta(hours=5, minutes=30)),
        timezone(timedelta(hours=-8)), timezone(timedelta(seconds=1, microseconds=5), "odd"),
        timezone(timedelta(hours=23, minutes=59, seconds=59, microseconds=999999)),
        timezone(-timedelta(hours=23, minutes=59, seconds=59, microseconds=999999)),
        Fixed(0), Fixed(-300), Fixed(90), FoldTz(), NoneTz(), BadTz(timedelta(hours=24)), BadTz(5),
        BadTz(Sub(hours=2)), BadTz(-timedelta(days=1)), RaiseTz()] + ZONES * 2)
YEARS = [1, 2, 1969, 1970, 2024, 2026, 2026, 2026, 9998, 9999]

def rdt():
    if rng.random() < 0.3:
        # Near a zone transition (New York, London 2026).
        base = rng.choice([datetime(2026, 3, 8, 1, 30), datetime(2026, 11, 1, 1, 30),
                           datetime(2026, 3, 29, 1, 30), datetime(2026, 10, 25, 1, 30),
                           datetime(2026, 4, 5, 1, 45), datetime(2026, 10, 4, 1, 45)])
        dt = base + timedelta(minutes=rng.randrange(-90, 91))
    else:
        y = rng.choice(YEARS)
        m = rng.randrange(1, 13)
        dt = datetime(y, m, rng.randrange(1, 29) if rng.random() < 0.8 else (31 if m in (1, 12) else 28),
                      rng.choice([0, 0, 23, rng.randrange(24)]), rng.randrange(60), rng.randrange(60),
                      rng.choice([0, 0, 999999, rng.randrange(1000000)]))
    dt = dt.replace(tzinfo=rng.choice(TZS), fold=rng.choice([0, 0, 1]))
    if rng.random() < 0.1:
        dt = MyDT(dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second, dt.microsecond, dt.tzinfo, fold=dt.fold)
    return dt

def rtd():
    return rng.choice([timedelta(0), timedelta(hours=1), timedelta(days=-1, microseconds=1),
                       timedelta(days=rng.randrange(-400, 400), seconds=rng.randrange(86400),
                                 microseconds=rng.randrange(1000000)),
                       timedelta(days=3652058), -timedelta(days=3652058), Sub(minutes=30)])

def show(f):
    try:
        r = f()
    except Exception as e:
        return f"{type(e).__name__}: {e}"
    return repr(r)

seen = {}
for i in range(3000):
    a, b = rdt(), (rdt() if rng.random() < 0.7 else None)
    if b is None:
        b = a.replace(tzinfo=rng.choice(TZS)) if rng.random() < 0.5 else a.replace(fold=1 - a.fold)
    td = rtd()
    out = [repr(a), repr(b)]
    out += [show(lambda: a == b), show(lambda: a != b), show(lambda: a < b), show(lambda: a <= b),
            show(lambda: a > b), show(lambda: a >= b)]
    out += [show(lambda: a - b), show(lambda: a + td), show(lambda: a - td), show(lambda: td + a)]
    out += [show(lambda: a.utcoffset()), show(lambda: a.isoformat()), show(lambda: str(a)),
            show(lambda: a.isoformat(' ', 'milliseconds'))]
    tz = rng.choice(TZS)
    out += [show(lambda: a.astimezone(tz)), show(lambda: a.astimezone(timezone.utc))]
    # hash: consistent with ==, compared within each engine only.
    try:
        ha, hb = hash(a), hash(b)
        eq = a == b
        out.append(f"hash-consistent {(not eq) or ha == hb}")
    except Exception as e:
        out.append(f"hash {type(e).__name__}: {e}")
    print(i, " | ".join(out))
