# ord() takes a bytes or bytearray of length 1 (and str/bytes subclasses);
# copy.copy/deepcopy of timezones and of tz-aware datetimes work (tzinfo's
# reduction asked for `object.__getstate__`, and the datetime state
# round-trip needs ord on bytes).
for x in (b'a', bytearray(b'\xff'), b'', b'ab', bytearray(), 'é', ''):
    try:
        print(ord(x))
    except TypeError as e:
        print(e)
class S(str): pass
class B(bytes): pass
print(ord(S('z')), ord(B(b'q')), [ord(b[i:i + 1]) for b in [b'xyz'] for i in range(3)])

import copy
from datetime import datetime, timezone, timedelta, date, time, tzinfo
tz = timezone(timedelta(hours=2), 'X')
print(copy.copy(tz), copy.deepcopy(tz), copy.deepcopy({'a': datetime(2024, 1, 1, tzinfo=tz)}))
print(copy.deepcopy(timezone.utc) is timezone.utc, copy.copy(tz) == tz, copy.deepcopy(tz) == tz)
print(copy.deepcopy(date(2020, 1, 2)), copy.copy(time(1, 2, tzinfo=tz)), copy.deepcopy(timedelta(3, 4)))
events = [{'at': datetime(2024, 5, 1, 9, 30, tzinfo=timezone.utc), 'tags': ['x']}]
snap = copy.deepcopy(events)
snap[0]['tags'].append('y')
print(snap, events[0]['tags'], snap[0]['at'] == events[0]['at'])

class Fixed(tzinfo):
    def __init__(self, h=0):
        self.h = h
    def utcoffset(self, dt):
        return timedelta(hours=self.h)
    def tzname(self, dt):
        return f'F{self.h}'
    def dst(self, dt):
        return timedelta(0)
f = copy.deepcopy(Fixed(3))
print(type(f).__name__, f.h, datetime(2024, 1, 1, tzinfo=f).isoformat())
