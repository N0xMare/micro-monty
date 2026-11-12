# Aware datetime fast paths: comparison, hash, subtraction, addition,
# utcoffset, astimezone and fromutc must keep CPython's semantics for fixed
# offsets, zoneinfo zones across transitions, fold, naive/aware mixes and
# tzinfo subclasses written in Python (which return None, raise, return bad
# types or out-of-range offsets).
from datetime import datetime, timedelta, timezone, tzinfo
from zoneinfo import ZoneInfo


def show(f, *a):
    try:
        return repr(f(*a))
    except Exception as e:
        return f'{type(e).__name__}: {e}'


class Fixed(tzinfo):
    def __init__(self, m):
        self.m = m

    def utcoffset(self, dt):
        return timedelta(minutes=self.m)

    def dst(self, dt):
        return timedelta(0)

    def tzname(self, dt):
        return f'F{self.m}'

    def __repr__(self):
        return f'Fixed({self.m})'


class FoldTz(tzinfo):
    def utcoffset(self, dt):
        return timedelta(hours=-4 if dt is not None and dt.fold else -5)

    def dst(self, dt):
        return timedelta(0)

    def __repr__(self):
        return 'FoldTz()'


class NoneTz(tzinfo):
    def utcoffset(self, dt):
        return None

    def __repr__(self):
        return 'NoneTz()'


class RaiseTz(tzinfo):
    def utcoffset(self, dt):
        raise KeyError('boom')

    def __repr__(self):
        return 'RaiseTz()'


class BadTz(tzinfo):
    def __init__(self, v):
        self.v = v

    def utcoffset(self, dt):
        return self.v

    def dst(self, dt):
        return self.v

    def __repr__(self):
        return f'BadTz({self.v!r})'


class TD(timedelta):
    pass


class DT(datetime):
    pass


NY = ZoneInfo('America/New_York')
LON = ZoneInfo('Europe/London')
UTC = timezone.utc
P530 = timezone(timedelta(hours=5, minutes=30), 'IST')
M8 = timezone(timedelta(hours=-8))
ops = [
    ('==', lambda a, b: a == b), ('!=', lambda a, b: a != b), ('<', lambda a, b: a < b),
    ('<=', lambda a, b: a <= b), ('>', lambda a, b: a > b), ('-', lambda a, b: a - b),
]

vals = [
    datetime(2026, 3, 9, 10, 30),
    datetime(2026, 3, 9, 10, 30, tzinfo=UTC),
    datetime(2026, 3, 9, 16, 0, tzinfo=P530),
    datetime(2026, 3, 9, 2, 30, tzinfo=M8),
    datetime(2026, 3, 9, 6, 30, tzinfo=NY),
    datetime(2026, 3, 9, 10, 30, tzinfo=Fixed(0)),
    datetime(2026, 3, 9, 5, 30, tzinfo=FoldTz()),
    datetime(2026, 3, 9, 6, 30, fold=1, tzinfo=FoldTz()),
    datetime(2026, 3, 9, 10, 30, tzinfo=NoneTz()),
    DT(2026, 3, 9, 11, 30, 0, 1, tzinfo=LON),
]
for a in vals:
    print(repr(a), show(a.utcoffset), show(hash, a) == show(hash, a))
    for b in vals:
        print(' ', [n + ' ' + show(f, a, b) for n, f in ops])

# Equal instants hash equally; naive and aware never compare equal.
inst = [v for v in vals if v.utcoffset() is not None]
for a in inst:
    for b in inst:
        if a == b:
            assert hash(a) == hash(b), (a, b)
print(len({*inst}), vals[0] == vals[8], vals[8] == vals[0], hash(vals[0]) == hash(vals[8]))

# Ambiguous and missing local times across New York and London transitions.
for tz in (NY, LON):
    for s in ('2026-03-08T02:30', '2026-11-01T01:30', '2026-03-29T01:30', '2026-10-25T01:30', '2026-07-01T12:00'):
        for fold in (0, 1):
            d = datetime.fromisoformat(s).replace(tzinfo=tz, fold=fold)
            u = d.astimezone(UTC)
            print(tz, s, fold, d.utcoffset(), d.dst(), d.tzname(), u, u.astimezone(tz), repr(u.astimezone(tz)),
                  d == u, hash(d) == hash(u), d - u, d - datetime(2026, 1, 1, tzinfo=tz))
    a = datetime(2026, 11, 1, 1, 30, tzinfo=tz)
    b = a.replace(fold=1)
    print(a == b, a < b, b - a, hash(a) == hash(b), a.astimezone(UTC), b.astimezone(UTC))
for h in range(-2, 4):
    u = datetime(2026, 11, 1, 5, tzinfo=UTC) + timedelta(hours=h)
    print(repr(u.astimezone(NY)), repr(u.astimezone(LON)), repr(NY.fromutc(u.replace(tzinfo=NY))))

# Fold for Python tzinfos: inter-zone comparisons see the fold, same-tz ones do not.
f0 = datetime(2026, 3, 9, 5, tzinfo=FoldTz())
f1 = f0.replace(fold=1)
print(f0 == f1, f0 < f1, f1 - f0, hash(f0) == hash(f1), f0.astimezone(UTC), f1.astimezone(UTC), repr(f1))
print(f0 == f1.astimezone(UTC), f1 == f1.astimezone(UTC), f1.astimezone(UTC) - f0)

# Arithmetic keeps the tzinfo and the subclass; fold resets.
for d in (vals[4], vals[6], vals[7], vals[9]):
    for td in (timedelta(days=1, seconds=7), -timedelta(microseconds=1), TD(hours=36)):
        print(repr(d + td), repr(td + d), repr(d - td), type(d + td).__name__)
print(show(lambda: datetime.max.replace(tzinfo=UTC) + timedelta(1)),
      show(lambda: datetime.min.replace(tzinfo=UTC) - timedelta(1)),
      show(lambda: datetime.max.replace(tzinfo=UTC) - datetime.min.replace(tzinfo=M8)))

# Bad offsets, raising tzinfos and odd types.
bad = [None, RaiseTz(), BadTz(5), BadTz(timedelta(hours=24)), BadTz(-timedelta(hours=24)), BadTz(TD(minutes=90)),
       BadTz(timedelta(seconds=1, microseconds=5)), BadTz(timedelta(hours=-23, minutes=-59))]
ref = datetime(2026, 3, 9, 10, 30, tzinfo=UTC)
for tz in bad:
    d = datetime(2026, 3, 9, 10, 30, tzinfo=tz)
    print(repr(tz), show(d.utcoffset), show(d.dst), show(lambda: d == ref), show(lambda: ref == d), show(lambda: d < ref),
          show(lambda: d - ref), show(hash, d) == show(hash, d), show(d.isoformat), show(d.astimezone, UTC),
          show(UTC.fromutc, d), show(lambda: Fixed(30).fromutc(d.replace(tzinfo=Fixed(30)))))
r = datetime(2026, 3, 9, tzinfo=RaiseTz())
print(show(lambda: r == r), show(lambda: r < r), show(lambda: r - r), show(hash, r))
print(show(lambda: vals[0] < vals[1]), show(lambda: vals[0] - vals[1]), show(lambda: vals[1] >= vals[0]))
print(show(Fixed(30).fromutc, vals[0]), show(Fixed(30).fromutc, 5), show(Fixed(30).fromutc, vals[1]),
      show(UTC.fromutc, vals[0]), show(NoneTz().fromutc, vals[0].replace(tzinfo=NoneTz())))
print(show(lambda: vals[1] == 5), show(lambda: vals[1] < 5), vals[1] == vals[1].date(),
      show(lambda: vals[1] < vals[1].date()), vals[1] != object())

# Fixed-offset formatting, astimezone between zones and to Python tzinfos.
for d in vals[1:8] + [vals[9]]:
    print(d.isoformat(), str(d), d.isoformat(timespec='minutes'), repr(d.astimezone(P530)), repr(d.astimezone(Fixed(-90))),
          repr(d.astimezone(LON)), d.timetz(), d.utctimetuple()[:6])
for us in (0, 1, 999999):
    t = timezone(timedelta(hours=-3, minutes=-7, seconds=-9, microseconds=-us))
    d = datetime(2026, 3, 9, tzinfo=t)
    print(d.isoformat(), d.astimezone(UTC), repr(t), str(t), d - datetime(2026, 3, 9, tzinfo=UTC))
