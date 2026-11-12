"""Date and time arithmetic over calendars, deployments, issues and orders."""

import calendar
import datetime as dt
from collections import Counter, defaultdict
from zoneinfo import ZoneInfo

from ptc import Transcript, r2, task
from tasks.worlds import (CUSTOMERS, DEPLOYS, EVENTS, HOLIDAYS, ISSUES, ORDERS, SUBSCRIPTIONS, USERS_TZ)

C = 'datetime-arithmetic'
NOW = dt.datetime(2026, 3, 15, 12, tzinfo=dt.UTC)
P = dt.datetime.fromisoformat


def evs(user=None, lo='2000-01-01T00:00:00+00:00', hi='2100-01-01T00:00:00+00:00'):
    out = [e for e in EVENTS if (user is None or user in e['attendees']) and P(lo) <= P(e['start']) < P(hi)]
    return sorted(out, key=lambda e: (P(e['start']), e['id']))


wk = evs('alice', '2026-03-16T00:00:00+00:00', '2026-03-23T00:00:00+00:00')
task('dt-meeting-hours', C, """
How many hours of meetings does alice have in the week starting Monday 2026-03-16 (events that start
from 2026-03-16T00:00Z up to but excluding 2026-03-23T00:00Z)?
""", ['list_events'],
    answer={'hours': ('number', 'Total duration in hours, rounded to 2 decimals'), 'events': ('integer', 'Number of events')},
    expect={'hours': r2(sum((P(e['end']) - P(e['start'])).total_seconds() for e in wk) / 3600), 'events': len(wk)}, tol=0.006)

kol = ZoneInfo('Asia/Kolkata')
day17 = evs('chitra', '2026-03-17T00:00:00+00:00', '2026-03-18T00:00:00+00:00')
task('dt-local-times', C, """
List chitra's events that start on 2026-03-17 (UTC), with their start time converted to chitra's own
time zone (see her profile).
""", ['list_events', 'get_user_profile'],
    answer={'events': ('array', 'List of {"id": str, "local_start": "YYYY-MM-DD HH:MM"}, in start order')},
    expect={'events': [{'id': e['id'], 'local_start': P(e['start']).astimezone(kol).strftime('%Y-%m-%d %H:%M')} for e in day17]})

tokyo = ZoneInfo('Asia/Tokyo')
emi = evs('emi', '2026-03-09T00:00:00+00:00', '2026-03-23T00:00:00+00:00')
outside = [e['id'] for e in emi if not ('09:30' <= P(e['start']).astimezone(tokyo).strftime('%H:%M') < '18:30')]
task('dt-outside-hours', C, """
Which of emi's events between 2026-03-09T00:00Z and 2026-03-23T00:00Z start outside her working hours
(start time before the start or at/after the end of her working hours, in her local time zone)?
""", ['list_events', 'get_user_profile'],
    answer={'event_ids': ('array', 'Event ids in start order')},
    expect={'event_ids': outside})


def business_days(a, b, hols):
    n, d = 0, a
    while d <= b:
        if d.weekday() < 5 and d.isoformat() not in hols:
            n += 1
        d += dt.timedelta(days=1)
    return n


task('dt-business-days', C, """
How many business days (Monday to Friday, excluding US public holidays) are there from 2026-03-15 to
2026-07-15, both inclusive?
""", ['list_holidays'],
    answer={'business_days': ('integer', 'The count')},
    expect={'business_days': business_days(dt.date(2026, 3, 15), dt.date(2026, 7, 15), set(HOLIDAYS[('US', 2026)]))})

d, k = dt.date(2026, 3, 15), 0
while k < 30:
    d += dt.timedelta(days=1)
    if d.weekday() < 5 and d.isoformat() not in HOLIDAYS[('DE', 2026)]:
        k += 1
task('dt-due-date-de', C, """
An invoice issued in Germany on 2026-03-15 is due 30 business days later. Business days are Monday to
Friday excluding German public holidays; counting starts the day after issue. What is the due date?
""", ['list_holidays'],
    answer={'due_date': 'YYYY-MM-DD'},
    expect={'due_date': d.isoformat()})


def add_months(day, n):
    m = day.month - 1 + n
    y, m = day.year + m // 12, m % 12 + 1
    return dt.date(y, m, min(day.day, calendar.monthrange(y, m)[1]))


ren = []
for s in SUBSCRIPTIONS:
    if s['cancelled_at']:
        continue
    first = dt.date.fromisoformat(s['started']) + dt.timedelta(days=s['trial_days'])
    step = {'monthly': 1, 'quarterly': 3, 'annual': 12}[s['plan']]
    k = 0
    while add_months(first, k * step) <= dt.date(2026, 3, 15):
        k += 1
    ren.append({'id': s['id'], 'next_renewal': add_months(first, k * step).isoformat()})
task('dt-next-renewals', C, """
For every subscription that is not cancelled, compute the next renewal date strictly after 2026-03-15.
Billing starts on the first billing date = started + trial_days days. The k-th renewal is the first
billing date plus k billing periods (monthly = 1 month, quarterly = 3 months, annual = 12 months), where
adding months keeps the first billing date's day of month, clamped to the last day of the target month.
""", ['list_subscriptions'],
    answer={'renewals': ('array', 'List of {"id": str, "next_renewal": "YYYY-MM-DD"} in the listed order')},
    expect={'renewals': ren})

dur = defaultdict(list)
for x in DEPLOYS:
    dur[x['service']].append(x['duration_s'])
days = Counter(dt.datetime.fromtimestamp(x['started_at'], dt.UTC).date().isoformat() for x in DEPLOYS)
busiest = sorted(days.items(), key=lambda kv: (-kv[1], kv[0]))[0][0]
task('dt-deploy-stats', C, """
From the deployment list: the average deployment duration per service in minutes, and the UTC calendar
day with the most deployment starts (ties: earliest day).
""", ['list_deployments'],
    answer={'avg_minutes': ('object', 'Service -> average duration in minutes, rounded to 1 decimal'),
            'busiest_day': 'YYYY-MM-DD'},
    expect={'avg_minutes': {k: round(sum(v) / len(v) / 60, 1) for k, v in dur.items()}, 'busiest_day': busiest}, tol=0.06)

gaps = [(b['started_at'] - a['started_at'], a['id'], b['id']) for a, b in zip(DEPLOYS, DEPLOYS[1:])]
g = max(gaps)
task('dt-longest-gap', C, """
What was the longest gap between two consecutive deployment starts (any service)?
""", ['list_deployments'],
    answer={'hours': ('number', 'Gap in hours, rounded to 2 decimals'), 'before': 'Id of the earlier deployment',
            'after': 'Id of the later deployment'},
    expect={'hours': r2(g[0] / 3600), 'before': g[1], 'after': g[2]}, tol=0.006)

stale = sorted(i['key'] for i in ISSUES if i['status'] != 'closed'
               and NOW - P(i['updated'].replace('Z', '+00:00')) > dt.timedelta(days=14))
task('dt-stale-issues', C, """
It is now 2026-03-15T12:00:00Z. Which issues that are not closed have not been updated for more than 14
days?
""", ['search_issues'],
    answer={'keys': ('array', 'Issue keys sorted ascending')},
    expect={'keys': stale})

wd = Counter(calendar.day_name[dt.date.fromisoformat(o['order_date']).weekday()] for o in ORDERS
             if o['order_date'].startswith('2026'))
task('dt-orders-by-weekday', C, """
Count all orders placed in 2026 (any status) by the weekday of their order date.
""", ['query_database'],
    answer={'counts': ('object', 'English weekday name like "Monday" -> count; only weekdays that occur')},
    expect={'counts': dict(wd)})

aw = evs('alice', '2026-03-09T00:00:00+00:00', '2026-03-16T00:00:00+00:00')
pairs = sorted([a['id'], b['id']] for i, a in enumerate(aw) for b in aw[i + 1:]
               if P(a['start']) < P(b['end']) and P(b['start']) < P(a['end']))
pairs = [sorted(p) for p in pairs]
task('dt-overlaps', C, """
Find double bookings: pairs of alice's events starting between 2026-03-09T00:00Z and 2026-03-16T00:00Z
whose time ranges overlap (touching end/start does not count).
""", ['list_events'],
    answer={'pairs': ('array', 'List of [id1, id2] with id1 < id2, the list sorted ascending; empty if none')},
    expect={'pairs': sorted(pairs)})

failed = [{'id': x['id'], 'service': x['service'],
           'started': dt.datetime.fromtimestamp(x['started_at'], dt.UTC).strftime('%Y-%m-%dT%H:%M:%SZ')}
          for x in DEPLOYS if x['status'] == 'failed']
task('dt-epoch-to-iso', C, """
List the failed deployments with their start time as an ISO 8601 UTC string like 2026-03-04T17:05:09Z.
""", ['list_deployments'],
    answer={'failed': ('array', 'List of {"id": str, "service": str, "started": str}, oldest first')},
    expect={'failed': failed})

busy = [e for e in EVENTS if set(e['attendees']) & {'alice', 'bjorn', 'diego'}]
slot = None
t = dt.datetime(2026, 3, 18, 13, tzinfo=dt.UTC)
while t + dt.timedelta(hours=1) <= dt.datetime(2026, 3, 18, 17, tzinfo=dt.UTC):
    if all(not (P(e['start']) < t + dt.timedelta(hours=1) and t < P(e['end'])) for e in busy):
        slot = t
        break
    t += dt.timedelta(minutes=15)


def check_slot(tr: Transcript):
    calls = tr.to('create_event')
    if len(calls) != 1:
        return f'expected one create_event call, got {len(calls)}'
    c = calls[0]
    try:
        s, e = P(c['start'].replace('Z', '+00:00')), P(c['end'].replace('Z', '+00:00'))
    except (KeyError, ValueError, AttributeError) as err:
        return f'bad event times: {err}'
    if s.tzinfo is None:
        s, e = s.replace(tzinfo=dt.UTC), e.replace(tzinfo=dt.UTC)
    if (s, e) != (slot, slot + dt.timedelta(hours=1)):
        return f'expected slot {slot.isoformat()}, got {c["start"]}..{c["end"]}'
    if sorted(c.get('attendees') or []) != ['alice', 'bjorn', 'diego'] or c.get('title') != 'Sync':
        return f'wrong title/attendees: {c.get("title")!r} {c.get("attendees")!r}'
    return None


task('dt-free-slot', C, """
Schedule a 60-minute meeting titled "Sync" for alice, bjorn and diego on 2026-03-18 at the earliest time
between 13:00 and 17:00 UTC (it must end by 17:00) when none of them has an event. Candidate start times
are on 15-minute boundaries. Create the event with create_event, attendees ["alice", "bjorn", "diego"].
""", ['list_events', 'create_event'], check=check_slot)

wk_rev = defaultdict(float)
for o in ORDERS:
    if o['status'] == 'completed' and '2026-01-01' <= o['order_date'] <= '2026-03-31':
        y, w, _ = dt.date.fromisoformat(o['order_date']).isocalendar()
        wk_rev[f'{y}-W{w:02d}'] += o['total']
task('dt-iso-week-revenue', C, """
Total the revenue of completed orders placed in Q1 2026 (2026-01-01 to 2026-03-31) per ISO week.
""", ['query_database'],
    answer={'weeks': ('object', 'ISO week label like "2026-W03" (ISO year, zero-padded week) -> revenue rounded to 2 decimals')},
    expect={'weeks': {k: r2(v) for k, v in sorted(wk_rev.items())}}, tol=0.006)

qs = Counter(f'{c["signup_date"][:4]}-Q{(int(c["signup_date"][5:7]) - 1) // 3 + 1}' for c in CUSTOMERS)
task('dt-signup-quarters', C, """
How many customers signed up in each calendar quarter?
""", ['list_customers'],
    answer={'quarters': ('object', 'Label like "2024-Q3" -> number of signups; only quarters that occur')},
    expect={'quarters': dict(qs)})

wkend = sorted(i['key'] for i in ISSUES if P(i['created'].replace('Z', '+00:00')).weekday() >= 5)
task('dt-weekend-issues', C, """
Which issues (any status) were created on a Saturday or Sunday (UTC)?
""", ['search_issues'],
    answer={'keys': ('array', 'Issue keys sorted ascending')},
    expect={'keys': wkend})

per_author = defaultdict(int)
for x in DEPLOYS:
    per_author[x['author']] += x['duration_s']
task('dt-deploy-time-per-author', C, """
Total the deployment durations per author and format each total as H:MM:SS (hours not zero-padded,
minutes and seconds two digits, e.g. 1:05:09).
""", ['list_deployments'],
    answer={'totals': ('object', 'Author -> "H:MM:SS"')},
    expect={'totals': {a: f'{s // 3600}:{s % 3600 // 60:02d}:{s % 60:02d}' for a, s in per_author.items()}})
