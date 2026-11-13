# email.utils's date functions: RFC 2822 dates as feeds and mail headers
# write them.
from datetime import timezone
from email.utils import (format_datetime, formatdate, mktime_tz, parsedate,
                         parsedate_to_datetime, parsedate_tz)
import email.utils

for s in ['Mon, 02 Mar 2026 10:00:00 +0100', 'Mon, 02 Mar 2026 10:00:00 GMT',
          'Tue, 10 Mar 2026 09:30:00 -0000', '2 Mar 26 10:00 EST', 'Mon,2 Mar 2026 10:00:05 PDT',
          '02 Mar 2026 10:00:00 Z', 'Mar 02 2026 10:00:00 +0530', '2-Mar-26 10:00:00 GMT',
          'Mon, 02 Mar 2026 10.00.30 +0000', 'Wed, 31 Dec 69 23:59 UT']:
    d = parsedate_to_datetime(s)
    print(repr(s), d, d.tzinfo, parsedate(s), parsedate_tz(s))
    if d.tzinfo is not None:
        print('  ', d.astimezone(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'), format_datetime(d),
              mktime_tz(parsedate_tz(s)))
for bad in ['', '   ', 'garbage', 'Mon, 99 Foo 2026', 'Mon, 02 Mar 2026', 'Mon, 31 Feb 2026 10:00:00 GMT']:
    print(repr(bad), parsedate(bad), parsedate_tz(bad))
    try:
        print(parsedate_to_datetime(bad))
    except ValueError as e:
        print('  ValueError', e)
d = email.utils.parsedate_to_datetime('Sat, 14 Mar 2026 08:15:00 +0000')
print(format_datetime(d), format_datetime(d.astimezone(timezone.utc), usegmt=True))
print(format_datetime(d.replace(tzinfo=None)))
try:
    format_datetime(d.replace(tzinfo=None), usegmt=True)
except ValueError as e:
    print(e)
print(formatdate(0), formatdate(1773576000.5, usegmt=True))
