"""Log and text triage over service logs and the access log."""

import re
from collections import Counter, defaultdict

from ptc import Transcript, compare, task
from tasks.worlds import ACCESS_LOG, LOG_DATES, LOGS, SERVICES, parse_log_line

C = 'log-text-triage'


def entries(service, date):
    return [parse_log_line(line) for line in LOGS[(service, date)]]


def kv(msg):
    return dict(re.findall(r'(\w+)=(\S+)', msg))


task('logs-errors-per-service', C, """
Count the ERROR lines in each service's log for 2026-03-14.
""", ['list_services', 'fetch_logs'],
    answer={'counts': ('object', 'Service name -> number of ERROR lines (include services with 0)')},
    expect={'counts': {s: sum(e[1] == 'ERROR' for e in entries(s, '2026-03-14')) for s in SERVICES}})

up = Counter()
for s in SERVICES:
    for d in LOG_DATES:
        for e in entries(s, d):
            m = re.match(r'upstream timeout calling (\w+)', e[3])
            if m:
                up[m.group(1)] += 1
task('logs-upstream-timeouts', C, """
Logs contain lines like "upstream timeout calling search after 3000ms". Across every service and all
three days of logs (2026-03-13 to 2026-03-15), how many timeouts were there per upstream service?
""", ['list_services', 'fetch_logs'],
    answer={'timeouts': ('object', 'Upstream service name -> number of timeout lines naming it')},
    expect={'timeouts': dict(up)})

reqs = []
for e in entries('auth', '2026-03-14'):
    if e[3].startswith('request completed'):
        f = kv(e[3])
        reqs.append((-int(f['latency_ms']), e[0], f['path']))
reqs.sort()
task('logs-slowest-requests', C, """
In the auth service log for 2026-03-14, find the five slowest completed requests (by latency_ms).
Break ties by earlier timestamp.
""", ['fetch_logs'],
    answer={'slowest': ('array', 'List of {"ts": timestamp string as in the log, "path": str, "latency_ms": int}, '
                        'slowest first')},
    expect={'slowest': [{'ts': ts, 'path': p, 'latency_ms': -lat} for lat, ts, p in reqs[:5]]})

ips = Counter()
for d in LOG_DATES:
    for e in entries('auth', d):
        if e[3].startswith('login failed'):
            ips[kv(e[3])['ip']] += 1
bad = sorted(((ip, n) for ip, n in ips.items() if n >= 3), key=lambda x: (-x[1], x[0]))
task('logs-failed-login-ips', C, """
Look for brute-force attempts: in the auth service logs for all three days (2026-03-13 to
2026-03-15), which IP addresses have 3 or more "login failed" lines?
""", ['fetch_logs'],
    answer={'ips': ('array', 'List of {"ip": str, "failures": int}, most failures first, ties by IP string ascending')},
    expect={'ips': [{'ip': ip, 'failures': n} for ip, n in bad]})

hours = defaultdict(lambda: [0, 0])
for e in entries('gateway', '2026-03-14'):
    if e[3].startswith('request completed'):
        h = e[0][11:13]
        hours[h][0] += 1
        hours[h][1] += int(kv(e[3])['status']) >= 500
task('logs-hourly-5xx-rate', C, """
For the gateway service on 2026-03-14, compute the share of completed requests that returned a 5xx
status, per UTC hour. Only include hours that have at least one completed request.
""", ['fetch_logs'],
    answer={'rates': ('object', 'Two-digit hour like "07" -> percentage of 5xx responses, rounded to 1 decimal')},
    expect={'rates': {h: round(v[1] / v[0] * 100, 1) for h, v in sorted(hours.items())}}, tol=0.06)

first = next(e[0] for e in entries('billing', '2026-03-15') if e[1] == 'ERROR' and e[0][11:19] >= '06:00:00')
task('logs-first-error-after', C, """
What is the timestamp of the first ERROR in the billing log of 2026-03-15 at or after 06:00 UTC, and
what does its message say?
""", ['fetch_logs'],
    answer={'ts': 'The timestamp exactly as in the log', 'message': 'The message text after "billing[pid]: "'},
    expect={'ts': first, 'message': next(e[3] for e in entries('billing', '2026-03-15') if e[0] == first)})

exc = Counter()
for s in SERVICES:
    for e in entries(s, '2026-03-13'):
        m = re.match(r'unhandled exception in worker: (\w+):', e[3])
        if m:
            exc[m.group(1)] += 1
task('logs-exception-classes', C, """
On 2026-03-13, workers across all services logged "unhandled exception in worker: <ExceptionClass>: ..."
lines. Count them by exception class.
""", ['list_services', 'fetch_logs'],
    answer={'counts': ('object', 'Exception class name -> count')},
    expect={'counts': dict(exc)})

ACC = re.compile(r'(\S+) - - \[([^\]]+)\] "(\S+) (\S+) [^"]*" (\d{3}) (\d+) "([^"]*)" "([^"]*)"')
acc = [ACC.match(line).groups() for line in ACCESS_LOG]
task('logs-access-status-classes', C, """
Summarize the web access log of 2026-03-14 by status class.
""", ['get_access_log'],
    answer={'classes': ('object', 'Status class "2xx", "3xx", "4xx", "5xx" -> request count (all four keys)')},
    expect={'classes': {f'{k}xx': sum(a[4][0] == str(k) for a in acc) for k in (2, 3, 4, 5)}})

top_ips = sorted(Counter(a[0] for a in acc).items(), key=lambda x: (-x[1], x[0]))[:3]
task('logs-access-top-ips', C, """
Which three client IPs made the most requests in the 2026-03-14 access log? Break ties by IP string.
""", ['get_access_log'],
    answer={'top': ('array', 'List of {"ip": str, "requests": int}, most first')},
    expect={'top': [{'ip': ip, 'requests': n} for ip, n in top_ips]})

task('logs-access-404-paths', C, """
Which distinct paths returned 404 in the 2026-03-14 access log? Ignore query strings (the part from
"?" on).
""", ['get_access_log'],
    answer={'paths': ('array', 'Distinct paths without query strings, sorted ascending')},
    expect={'paths': sorted({a[3].split('?')[0] for a in acc if a[4] == '404'})})


def family(ua):
    low = ua.lower()
    if 'bot' in low:
        return 'bot'
    if low.startswith(('curl', 'python-requests')):
        return 'script'
    return 'browser'


fam = Counter()
for a in acc:
    fam[family(a[7])] += int(a[5])
task('logs-bytes-by-agent', C, """
Classify each request in the 2026-03-14 access log by its user agent: "bot" if the user agent
contains "bot" (case-insensitive), else "script" if it starts with "curl" or "python-requests", else
"browser". Total the response bytes per class.
""", ['get_access_log'],
    answer={'bytes': ('object', '"bot", "script", "browser" -> total bytes')},
    expect={'bytes': dict(fam)})

err15 = {s: sum(e[1] == 'ERROR' for e in entries(s, '2026-03-15')) for s in SERVICES}
hot = sorted(s for s, n in err15.items() if n > 12)


def check_incident(t: Transcript):
    calls = t.to('create_incident')
    if len(calls) != 1:
        return f'expected one create_incident call, got {len(calls)}'
    c = calls[0]
    return compare({'title': 'Elevated errors on 2026-03-15', 'severity': 'sev2', 'services': hot},
                   {k: c.get(k) for k in ('title', 'severity', 'services')}, 1e-9)


task('logs-open-incident', C, """
Check every service's log for 2026-03-15. If any service has more than 12 ERROR lines that day, open
exactly one incident with title "Elevated errors on 2026-03-15", severity "sev2", and the affected
services (sorted alphabetically). If none qualify, do not open an incident.
""", ['list_services', 'fetch_logs', 'create_incident'],
    check=check_incident)

users = Counter()
for s in SERVICES:
    for e in entries(s, '2026-03-14'):
        if e[3].startswith('request completed'):
            users[kv(e[3])['user']] += 1
top_user = sorted(users.items(), key=lambda x: (-x[1], x[0]))[0]
task('logs-busiest-user', C, """
Which user made the most completed requests across all services on 2026-03-14? Break ties by the
user id string.
""", ['list_services', 'fetch_logs'],
    answer={'user': 'User id like "u17"', 'requests': ('integer', 'Their request count')},
    expect={'user': top_user[0], 'requests': top_user[1]})

paths5 = Counter()
for d in LOG_DATES:
    for e in entries('search', d):
        if e[3].startswith('request completed') and int(kv(e[3])['status']) >= 500:
            paths5[kv(e[3])['path']] += 1
task('logs-5xx-paths', C, """
Across the three days of the search service's logs, which request paths returned server errors
(status 500 or higher), and how often?
""", ['fetch_logs'],
    answer={'paths': ('object', 'Path -> number of 5xx responses')},
    expect={'paths': dict(paths5)})

slow = defaultdict(list)
for s in SERVICES:
    for e in entries(s, '2026-03-15'):
        m = re.match(r'slow query took (\d+)ms table=(\w+)', e[3])
        if m:
            slow[m.group(2)].append(int(m.group(1)))
task('logs-slow-query-tables', C, """
Summarize the "slow query took <N>ms table=<name>" warnings from all services on 2026-03-15 per table.
""", ['list_services', 'fetch_logs'],
    answer={'tables': ('object', 'Table name -> {"count": int, "max_ms": int}')},
    expect={'tables': {t: {'count': len(v), 'max_ms': max(v)} for t, v in slow.items()}})

window = [line for line, e in zip(LOGS[('gateway', '2026-03-14')], entries('gateway', '2026-03-14'))
          if '12:00:00' <= e[0][11:19] < '13:00:00' and e[1] in ('WARN', 'ERROR')]
task('logs-window-grep', C, """
Extract the WARN and ERROR lines of the gateway log for 2026-03-14 between 12:00:00 (inclusive) and
13:00:00 (exclusive) UTC.
""", ['fetch_logs'],
    answer={'lines': ('array', 'The matching log lines, exactly as they appear (no trailing newline), in log order')},
    expect={'lines': window})

norm = Counter(re.sub(r'\d+', 'N', e[3]) for e in entries('notifications', '2026-03-13') if e[1] == 'ERROR')
task('logs-dedupe-errors', C, """
Deduplicate the ERROR messages of the notifications service on 2026-03-13: normalize each message
(the text after "notifications[pid]: ") by replacing every run of digits with "N", then count the
distinct normalized messages.
""", ['fetch_logs'],
    answer={'messages': ('array', 'List of {"message": normalized str, "count": int}, highest count first, '
                         'ties by message ascending')},
    expect={'messages': [{'message': m, 'count': n} for m, n in sorted(norm.items(), key=lambda x: (-x[1], x[0]))]})
