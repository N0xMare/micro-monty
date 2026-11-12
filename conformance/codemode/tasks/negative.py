"""Tasks whose natural answer reaches past a small sandbox (docs/research/codemode-survey.md §8):
dataframe, HTTP-client, subprocess and timezone-database habits, more than 16 concurrent calls, and output
past an observation bound. Their checkers accept any correct answer; the census records what was used."""

import csv
import datetime as dt
import io
import statistics
from collections import defaultdict
from zoneinfo import ZoneInfo

from ptc import r2, task
from tasks.worlds import EVENTS, FILES, LOG_DATES, LOGS, ORDER_BY_ID, PAGES, SERVICES, metric_points

C = 'negative-habits'
import json  # noqa: E402

sales = list(csv.DictReader(io.StringIO(FILES['/data/sales_2026q1.csv'])))
piv = defaultdict(lambda: defaultdict(int))
for r in sales:
    piv[r['region']][r['date'][:7]] += int(r['units'])
task('neg-dataframe-pivot', C, """
Load /data/sales_2026q1.csv as a table and build a pivot table of total units with regions as rows and
months (YYYY-MM) as columns, filling missing cells with 0. Also give each region's share of all units.
""", ['read_file'],
    answer={'pivot': ('object', 'Region -> {month: units}, all three months present'),
            'share': ('object', 'Region -> percentage of all units, rounded to 1 decimal')},
    expect={'pivot': {g: {m: piv[g].get(m, 0) for m in ('2026-01', '2026-02', '2026-03')} for g in piv},
            'share': {g: round(sum(piv[g].values()) / sum(int(r['units']) for r in sales) * 100, 1) for g in piv}},
    tol=0.06)

status = json.loads(PAGES['https://api.example.com/v1/status'][2])
task('neg-http-get', C, """
Make a GET request to the status API at https://api.example.com/v1/status, parse the JSON response and
list the components that are not "operational", with their uptime.
""", ['http_get'],
    answer={'components': ('object', 'Component name -> uptime_pct, for non-operational components')},
    expect={'components': {c['name']: c['uptime_pct'] for c in status['components'] if c['status'] != 'operational'}})

doc = FILES['/docs/contract.txt']
task('neg-wc', C, """
Give me the output of `wc` for /docs/contract.txt: line count, word count and byte count.
""", ['read_file'],
    answer={'lines': ('integer', ''), 'words': ('integer', ''), 'bytes': ('integer', '')},
    expect={'lines': doc.count('\n'), 'words': len(doc.split()), 'bytes': len(doc.encode())})

ny = ZoneInfo('America/New_York')
ev = [e for e in EVENTS if '2026-03-09' <= e['start'][:10] <= '2026-03-10' and 'diego' in e['attendees']]
ev = sorted(ev, key=lambda e: (dt.datetime.fromisoformat(e['start']), e['id']))
task('neg-tz-names', C, """
List diego's events whose start date as written in the event (the YYYY-MM-DD part) is 2026-03-09 or
2026-03-10, with their start converted to New York local time (America/New_York; daylight saving time
started on 2026-03-08), in start order.
""", ['list_events'],
    answer={'events': ('array', 'List of {"id": str, "start_ny": "YYYY-MM-DD HH:MM"}')},
    expect={'events': [{'id': e['id'], 'start_ny': dt.datetime.fromisoformat(e['start']).astimezone(ny)
                        .strftime('%Y-%m-%d %H:%M')} for e in ev]})

ids = list(range(1001, 1041))
task('neg-fanout-40', C, """
Fetch orders 1001 through 1040, all at once in parallel, and report the sum of their totals and how many
are refunded.
""", ['get_order'],
    answer={'total': ('number', 'Rounded to 2 decimals'), 'refunded': ('integer', '')},
    expect={'total': r2(sum(ORDER_BY_ID[i]['total'] for i in ids)),
            'refunded': sum(ORDER_BY_ID[i]['status'] == 'refunded' for i in ids)}, tol=0.006)

task('neg-dump-logs', C, """
Print the full text of every service's log for all three days (2026-03-13 to 2026-03-15) so I can read
it, then submit the total number of log lines.
""", ['list_services', 'fetch_logs'],
    answer={'lines': ('integer', 'Total number of log lines')},
    expect={'lines': sum(len(LOGS[(s, d)]) for s in SERVICES for d in LOG_DATES)})

ser = {m: [v for _, v in metric_points('web-02', m)] for m in ('cpu', 'mem', 'latency_ms')}
keep = [i for i in range(48) if all(ser[m][i] is not None for m in ser)]
cols = {m: [ser[m][i] for i in keep] for m in ser}
task('neg-matrix', C, """
For host web-02, fetch the cpu, mem and latency_ms series and compute the 3x3 matrix of Pearson
correlations between them, using only timestamps where all three are present.
""", ['get_metrics'],
    answer={'matrix': ('array', '3x3 nested list in the order cpu, mem, latency_ms; values rounded to 3 decimals')},
    expect={'matrix': [[round(statistics.correlation(cols[a], cols[b]), 3) if a != b else 1.0 for b in cols]
                       for a in cols]}, tol=0.0011)
