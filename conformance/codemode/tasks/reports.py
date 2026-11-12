"""Report formatting: exact text the caller asked for."""

import datetime as dt
import statistics
from collections import Counter, defaultdict

from ptc import Transcript, task
from tasks.worlds import (CITIES, CUSTOMER_BY_ID, ENDPOINTS, HOSTS, ISSUES, LOG_DATES, LOGS, ORDERS, PRODUCTS,
                          QUOTES, SERVICES, metric_points, parse_log_line)

C = 'report-formatting'
done = [o for o in ORDERS if o['status'] == 'completed']


def text_check(tool, field, want, **match):
    def check(t: Transcript):
        calls = [c for c in t.to(tool) if all(c.get(k) == v for k, v in match.items())]
        if not calls:
            return f'no {tool} call matching {match}'
        got = calls[-1].get(field)
        if not isinstance(got, str):
            return f'{field} is not a string'
        if got.rstrip('\n') != want.rstrip('\n'):
            gl, wl = got.rstrip('\n').split('\n'), want.rstrip('\n').split('\n')
            for i, (g, w) in enumerate(zip(gl, wl)):
                if g != w:
                    return f'line {i + 1}: expected {w!r}, got {g!r}'
            return f'expected {len(wl)} lines, got {len(gl)}'
        return None
    return check


reg = defaultdict(float)
for o in done:
    reg[CUSTOMER_BY_ID[o['customer_id']]['region']] += o['total']
md = '| Region | Revenue |\n|---|---:|\n' + ''.join(f'| {r} | ${v:,.2f} |\n' for r, v in sorted(reg.items(), key=lambda x: -x[1]))
task('rep-markdown-table', C, """
Post to #sales a Markdown table of completed-order revenue per region (all time), highest first, exactly in
this form (header, separator, one row per region, money with a $ sign, thousands separators and 2
decimals):
| Region | Revenue |
|---|---:|
| West | $12,345.67 |
""", ['query_database', 'post_message'],
    check=text_check('post_message', 'text', md, channel='#sales'))

hosts = sorted(HOSTS)
rows = []
for h in hosts:
    v = [x for _, x in metric_points(h, 'cpu') if x is not None]
    rows.append(f'{h:<8} {statistics.mean(v):>6.1f} {max(v):>6.1f}')
fixed = 'HOST       MEAN    MAX\n' + '\n'.join(rows) + '\n'
task('rep-fixed-width', C, """
Write /out/cpu.txt: a fixed-width table of every host's cpu series (non-null samples), sorted by host
name. The first line is exactly "HOST       MEAN    MAX". Each row is the host left-aligned in 8 columns, a
space, the mean right-aligned in 6 columns with 1 decimal, a space, the max right-aligned in 6 columns with
1 decimal. End the file with a newline.
""", ['list_hosts', 'get_metrics', 'write_file'],
    check=text_check('write_file', 'content', fixed, path='/out/cpu.txt'))

err = {s: sum(parse_log_line(x)[1] == 'ERROR' for x in LOGS[(s, '2026-03-15')]) for s in SERVICES}
total_err = sum(err.values())
lines = ['Daily error report for 2026-03-15', '']
for s, n in sorted(err.items(), key=lambda x: (-x[1], x[0])):
    lines.append(f'- {s}: {n} errors ({n / total_err:.1%})')
lines += ['', f'Total: {total_err}']
task('rep-daily-errors', C, """
Post a daily error report for 2026-03-15 to #ops. Format, exactly:
Daily error report for 2026-03-15
<blank line>
- <service>: <n> errors (<share of all errors as a percentage with 1 decimal, e.g. 12.5%>)
... one line per service, most errors first, ties by name ...
<blank line>
Total: <total>
""", ['list_services', 'fetch_logs', 'post_message'],
    check=text_check('post_message', 'text', '\n'.join(lines), channel='#ops'))

wt = '\n'.join(f'{c}: {CITIES[c][0]:.1f}°C, {CITIES[c][1]}' for c in sorted(CITIES))
task('rep-weather-digest', C, """
Get the weather for every city in London, Paris, Berlin, Madrid, Rome, Oslo, Tokyo, Sydney, New York,
Chicago, Austin and Toronto, and post to #general one line per city, sorted by city name, formatted as
"<city>: <temp with 1 decimal>°C, <condition>".
""", ['get_weather', 'post_message'],
    check=text_check('post_message', 'text', wt, channel='#general'))

quotes = ''.join(f'{s},{QUOTES[s][0]:.2f},{QUOTES[s][1]:+.2f}%\n' for s in sorted(QUOTES))
task('rep-quotes-csv', C, """
Write /out/quotes.csv with the header "symbol,price,change" and one row per symbol (ACME, GLBX, INIT, UMBR,
WAYN, STRK, OSCP, CYDN) sorted by symbol: price with 2 decimals, change as a signed percentage with 2
decimals and a % sign (e.g. +1.24%, -2.81%, +0.00%).
""", ['get_quote', 'write_file'],
    check=text_check('write_file', 'content', 'symbol,price,change\n' + quotes, path='/out/quotes.csv'))

open_by = Counter(i['priority'] for i in ISSUES if i['status'] != 'closed')
oldest = min((i for i in ISSUES if i['status'] != 'closed'), key=lambda i: (i['created'], i['key']))
age = (dt.datetime(2026, 3, 15, 12) - dt.datetime.fromisoformat(oldest['created'][:-1])).days
summ = (f'Open issues: {sum(open_by.values())}\n'
        + '\n'.join(f'  {p}: {open_by.get(p, 0)}' for p in ('P1', 'P2', 'P3', 'P4'))
        + f'\nOldest: {oldest['key']} ({age} days) - {oldest['title']}')
task('rep-issue-summary', C, """
It is 2026-03-15T12:00:00Z. Post to #eng a summary of the issues that are not closed, exactly:
Open issues: <count>
  P1: <n>
  P2: <n>
  P3: <n>
  P4: <n>
Oldest: <key> (<whole days since it was created, rounded down> days) - <title>
(the oldest is the one with the earliest created timestamp; ties by key)
""", ['search_issues', 'post_message'],
    check=text_check('post_message', 'text', summ, channel='#eng'))

st = []
for u in sorted(ENDPOINTS):
    up, ms = ENDPOINTS[u]
    st.append(f'{"✅" if up else "❌"} {u.removeprefix("https://")} ' + (f'{ms} ms' if up else 'DOWN'))
task('rep-status-page', C, """
Probe these endpoints: """ + ', '.join(sorted(ENDPOINTS)) + """. Post to #status one line per endpoint, sorted
by URL: "✅ <host> <latency> ms" when up or "❌ <host> DOWN" when down, where <host> is the URL without
"https://".
""", ['check_health', 'post_message'],
    check=text_check('post_message', 'text', '\n'.join(st), channel='#status'))

top = sorted(PRODUCTS, key=lambda p: (-p['price'] * p['stock'], p['id']))[:5]
body = 'Top 5 products by stock value:\n' + ''.join(
    f'{i}. {p["name"]} ({p["id"]}) - ${p["price"] * p["stock"]:,.2f}\n' for i, p in enumerate(top, 1))
task('rep-email-top-products', C, """
Email inventory@example.com with subject "Stock value report" and this body, exactly:
Top 5 products by stock value:
1. <name> (<id>) - $<price x stock with thousands separators and 2 decimals>
... five numbered lines, highest value first, ties by id ...
""", ['list_products', 'send_email'],
    check=text_check('send_email', 'body', body, to='inventory@example.com'))

mon = defaultdict(lambda: [0, 0.0])
for o in done:
    m = o['order_date'][:7]
    mon[m][0] += 1
    mon[m][1] += o['total']
ms = sorted(mon)
out = ['month    orders    revenue   change']
prev = None
for m in ms:
    n, v = mon[m]
    ch = '     n/a' if prev is None else f'{(v - prev) / prev:+8.1%}'
    out.append(f'{m} {n:>9} {v:>10,.0f} {ch}')
    prev = v
task('rep-monthly-change', C, """
Write /out/monthly.txt with one row per month of completed orders (oldest first). Header line exactly
"month    orders    revenue   change". Each row: the month as YYYY-MM, a space, the order count
right-aligned in 9 columns, a space, the revenue right-aligned in 10 columns with thousands separators and
no decimals, a space, then the change vs the previous month as a signed percentage with 1 decimal
right-aligned in 8 columns (e.g. "  +12.5%"), or "     n/a" for the first month.
""", ['query_database', 'write_file'],
    check=text_check('write_file', 'content', '\n'.join(out), path='/out/monthly.txt'))

day = {d: sum(parse_log_line(x)[1] == 'ERROR' for s in SERVICES for x in LOGS[(s, d)]) for d in LOG_DATES}
peak = max(day.values())
bars = '\n'.join(f'{d} {'#' * round(n / peak * 40):<40} {n}' for d, n in day.items())
task('rep-ascii-chart', C, """
Make an ASCII bar chart of the total number of ERROR lines per day over all services (2026-03-13 to
2026-03-15) and post it to #ops. One line per day, in date order: the date, a space, a bar of "#"
characters whose length is round(count / max_count * 40) padded with spaces to 40 characters, a space,
the count.
""", ['list_services', 'fetch_logs', 'post_message'],
    check=text_check('post_message', 'text', bars, channel='#ops'))

cust = CUSTOMER_BY_ID[7]
orders7 = sorted((o for o in ORDERS if o['customer_id'] == 7), key=lambda o: (o['order_date'], o['id']))
stmt = [f'Statement for {cust["name"]} <{cust["email"]}>', '']
stmt += [f'{o["order_date"]}  #{o["id"]}  {o["status"]:<9}  {o["total"]:>10,.2f}' for o in orders7]
stmt += ['', f'Completed total: {sum(o["total"] for o in orders7 if o["status"] == "completed"):,.2f} USD']
task('rep-statement', C, """
Write a statement for customer 7 to /out/statement_7.txt, exactly:
Statement for <name> <<email>>
<blank line>
one line per order, oldest first (ties by id): "<YYYY-MM-DD>  #<id>  <status left-aligned in 9>  <total
right-aligned in 10 with thousands separators and 2 decimals>" (two spaces between fields)
<blank line>
Completed total: <sum of completed totals with thousands separators and 2 decimals> USD
""", ['get_customer', 'list_orders', 'write_file'],
    check=text_check('write_file', 'content', '\n'.join(stmt), path='/out/statement_7.txt'))

js = '{\n  "hosts": ' + str(len(HOSTS)) + ',\n  "zones": [\n    "us-east-1a",\n    "us-east-1b",\n    "us-east-1c"\n  ]\n}'
task('rep-pretty-json', C, """
Write /out/cluster.json: a JSON object with "hosts" (the number of hosts) and "zones" (the distinct zones,
sorted), pretty-printed with 2-space indentation and keys in that order.
""", ['list_hosts', 'write_file'],
    check=text_check('write_file', 'content', js, path='/out/cluster.json'))

wrap_src = ('Our quarterly results exceeded expectations across all regions, driven by strong hardware sales '
            'and a growing services business. We thank every customer and partner for their continued trust.')
import textwrap  # noqa: E402

wrapped = textwrap.fill(wrap_src, width=50)
task('rep-wrap', C, f"""
Email ceo@example.com with subject "Quarterly note" and the following paragraph as the body, word-wrapped
to lines of at most 50 characters (break only at spaces, as greedily as possible, no trailing spaces):
{wrap_src}
""", ['send_email'],
    check=text_check('send_email', 'body', wrapped, to='ceo@example.com'))

counts = Counter(o['status'] for o in ORDERS)
n = len(ORDERS)
pct = ', '.join(f'{s} {counts[s] / n:.0%}' for s in ('completed', 'pending', 'cancelled', 'refunded'))
task('rep-one-liner', C, """
Post to #sales a one-line order status breakdown over all orders, exactly
"Orders: <total> (completed X%, pending X%, cancelled X%, refunded X%)", each percentage rounded to a
whole number.
""", ['query_database', 'post_message'],
    check=text_check('post_message', 'text', f'Orders: {n} ({pct})', channel='#sales'))
