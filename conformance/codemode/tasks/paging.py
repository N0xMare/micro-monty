"""Pagination (cursors, page numbers, offsets, tokens) and retries/error handling."""

from collections import Counter

from ptc import Transcript, compare, r2, task
from tasks.worlds import (ACCOUNTS, CITIES, CUSTOMERS, ENDPOINTS, HOSTS, ISSUES, LOG_DATES, LOGS, ORDERS, QUOTES,
                          SERVICES, TRANSACTIONS, metric_points, parse_log_line)

P = 'pagination'
R = 'retries-errors'

# ---------------------------------------------------------------- pagination
pending = sorted(o['id'] for o in ORDERS if o['status'] == 'pending')
task('page-all-pending', P, """
List every pending order (all pages; list_orders returns 25 per page with a next_cursor). Report how many
there are and their total value.
""", ['list_orders'],
    answer={'count': ('integer', 'Number of pending orders'), 'total': ('number', 'Sum of totals, rounded to 2 decimals')},
    expect={'count': len(pending), 'total': r2(sum(o['total'] for o in ORDERS if o['status'] == 'pending'))}, tol=0.006)

feb = [o for o in ORDERS if '2026-02-01' <= o['order_date'] <= '2026-02-28']
task('page-feb-orders', P, """
Using list_orders (paginated with next_cursor), count February 2026 orders by status.
""", ['list_orders'],
    answer={'counts': ('object', 'Status -> count, only statuses that occur')},
    expect={'counts': dict(Counter(o['status'] for o in feb))})

task('page-all-customers', P, """
Page through all customers with list_customers and report how many are in each country.
""", ['list_customers'],
    answer={'countries': ('object', 'Country code -> count')},
    expect={'countries': dict(Counter(c['country'] for c in CUSTOMERS))})

open_bugs = sorted(i['key'] for i in ISSUES if i['status'] != 'closed' and 'bug' in i['labels'])
task('page-open-bugs', P, """
Find all issues labelled "bug" that are not closed (search_issues returns 20 per page with has_more).
""", ['search_issues'],
    answer={'keys': ('array', 'Issue keys sorted ascending')},
    expect={'keys': open_bugs})

errs = [(e[0], e[3]) for d in LOG_DATES for e in map(parse_log_line, LOGS[('gateway', d)]) if e[1] == 'ERROR']
task('page-error-entries', P, """
Using query_logs (paginated with next_page_token), fetch every ERROR entry of the gateway service. Report
the count and the last entry's timestamp.
""", ['query_logs'],
    answer={'count': ('integer', 'Number of ERROR entries'), 'last_ts': 'Timestamp of the newest one'},
    expect={'count': len(errs), 'last_ts': errs[-1][0]})

frozen = [a['account_id'] for a in ACCOUNTS if a['status'] == 'frozen']
task('page-accounts-offset', P, """
Page through all bank accounts with list_accounts (offset/limit, at most 10 per page; the response has the
total). Report the frozen accounts and the total balance of active USD accounts.
""", ['list_accounts'],
    answer={'frozen': ('array', 'Frozen account ids in listed order'),
            'active_usd_total': ('number', 'Rounded to 2 decimals')},
    expect={'frozen': frozen, 'active_usd_total': r2(sum(a['balance'] for a in ACCOUNTS
                                                         if a['status'] == 'active' and a['currency'] == 'USD'))},
    tol=0.006)

c9 = sorted(o['id'] for o in ORDERS if o['customer_id'] == 9)
task('page-customer-orders', P, """
List all orders of customer 9 (follow next_cursor until it is null). Report their ids ascending.
""", ['list_orders'],
    answer={'order_ids': ('array', 'Ascending')},
    expect={'order_ids': c9})


def check_first_page_only(t: Transcript):
    n = len(t.to('list_orders'))
    if n > 1:
        return f'expected a single list_orders call, got {n}'
    return compare({'order_id': max(ORDERS, key=lambda o: (o['order_date'], o['id']))['id']}, t.answer() or {}, 1e-9)


task('page-newest-order', P, """
What is the id of the newest order? list_orders returns newest first; do not fetch more pages than needed.
""", ['list_orders'], answer={'order_id': ('integer', 'The id')}, check=check_first_page_only)

west = [c for c in CUSTOMERS if c['region'] == 'West']
target = sorted(west, key=lambda c: c['signup_date'])[0]
task('page-earliest-west', P, """
Among customers in the West region (list_customers supports a region filter and is paginated), who signed
up first?
""", ['list_customers'],
    answer={'name': 'Customer name', 'signup_date': 'YYYY-MM-DD'},
    expect={'name': target['name'], 'signup_date': target['signup_date']})

assignee = Counter(i['assignee'] or 'unassigned' for i in ISSUES if i['status'] == 'open')
task('page-open-by-assignee', P, """
Count open issues (status open) per assignee across all pages of search_issues. Use "unassigned" for
issues with no assignee.
""", ['search_issues'],
    answer={'counts': ('object', 'Assignee -> number of open issues')},
    expect={'counts': dict(assignee)})

big = next(o for o in sorted(ORDERS, key=lambda o: (o['order_date'], o['id']), reverse=True) if o['total'] > 3000)


def check_stop_early(t: Transcript):
    pos = sorted(ORDERS, key=lambda o: (o['order_date'], o['id']), reverse=True).index(big)
    need = pos // 25 + 1
    n = len(t.to('list_orders'))
    if n != need:
        return f'expected {need} list_orders calls (stop once found), got {n}'
    return compare({'order_id': big['id']}, t.answer() or {}, 1e-9)


task('page-stop-early', P, """
Find the most recent order with a total above 3000 USD. Page through list_orders (newest first) and stop
fetching pages as soon as you find one.
""", ['list_orders'], answer={'order_id': ('integer', 'The order id')}, check=check_stop_early)

per_level = Counter(parse_log_line(line)[1] for d in LOG_DATES for line in LOGS[('auth', d)]
                    if parse_log_line(line)[1] in ('WARN', 'ERROR'))
task('page-auth-warn-plus', P, """
Using query_logs with level WARN (which returns WARN and ERROR entries), page through all of the auth
service's entries and count them by level.
""", ['query_logs'],
    answer={'counts': ('object', 'Level -> count')},
    expect={'counts': dict(per_level)})

# ------------------------------------------------------------------- retries
TRANSIENT = [['TimeoutError', 'upstream timed out'], ['RuntimeError', '503 Service Unavailable']]
RETRY = ('Calls may fail transiently (TimeoutError or RuntimeError mentioning 503); retry a failing call up to 3 '
         'more times before giving up.')

task('retry-weather', R, f"""
Get the current temperature in Madrid. {RETRY}
""", ['get_weather'], answer={'temp_c': ('number', 'Temperature')},
    expect={'temp_c': CITIES['Madrid'][0]}, config={'fail': {'get_weather': {'Madrid': TRANSIENT}}})

task('retry-quotes', R, f"""
Fetch quotes for ACME, GLBX and INIT and report their prices. {RETRY}
""", ['get_quote'], answer={'prices': ('object', 'Symbol -> price')},
    expect={'prices': {s: QUOTES[s][0] for s in ('ACME', 'GLBX', 'INIT')}},
    config={'fail': {'get_quote': {'GLBX': TRANSIENT, 'INIT': TRANSIENT[:1]}}})


def check_giveup(t: Transcript):
    tries = [c for c in t.calls if c.name == 'check_health' and c.args.get('url') == 'https://cdn.example.com']
    if len(tries) != 4:
        return f'expected 4 attempts on the cdn endpoint, got {len(tries)}'
    want = {'up': sorted(u for u, v in ENDPOINTS.items() if v[0] and u != 'https://cdn.example.com'),
            'unreachable': ['https://cdn.example.com']}
    return compare(want, t.answer() or {}, 1e-9)


task('retry-give-up', R, f"""
Probe every endpoint in {', '.join(sorted(ENDPOINTS))}. {RETRY} An endpoint whose probe still fails after the
retries is "unreachable" (keep going with the others). Report which endpoints are up and which are
unreachable.
""", ['check_health'],
    answer={'up': ('array', 'URLs reported up, sorted'), 'unreachable': ('array', 'URLs whose probe kept failing, sorted')},
    check=check_giveup, config={'fail': {'check_health': {'https://cdn.example.com': TRANSIENT * 3}}})

task('retry-paged-accounts', R, f"""
Count all accounts per status by paging through list_accounts (offset/limit, at most 10 per page). {RETRY}
""", ['list_accounts'], answer={'counts': ('object', 'Status -> count')},
    expect={'counts': dict(Counter(a['status'] for a in ACCOUNTS))},
    config={'fail': {'list_accounts': {'20': TRANSIENT}}})

task('retry-logs', R, f"""
Count the ERROR lines of every service on 2026-03-15. {RETRY}
""", ['list_services', 'fetch_logs'],
    answer={'errors': ('object', 'Service -> ERROR count')},
    expect={'errors': {s: sum(parse_log_line(x)[1] == 'ERROR' for x in LOGS[(s, '2026-03-15')]) for s in SERVICES}},
    config={'fail': {'fetch_logs': {'billing': TRANSIENT[:1], 'search': TRANSIENT}}})


def check_not_found(t: Transcript):
    ids = [4, 77, 12, 300, 25]
    want = {'found': {str(i): next(c['name'] for c in CUSTOMERS if c['id'] == i) for i in ids if i <= 40},
            'not_found': [i for i in ids if i > 40]}
    retried = Counter(c.args.get('customer_id') for c in t.calls if c.name == 'get_customer')
    if any(retried[i] > 1 for i in (77, 300)):
        return 'a not-found lookup was retried; not-found is permanent'
    return compare(want, t.answer() or {}, 1e-9)


task('retry-permanent-errors', R, """
Look up customers 4, 77, 12, 300 and 25. A missing customer raises LookupError ("404 Not Found"): that is
permanent, so do not retry it. Report the names found and the ids not found.
""", ['get_customer'],
    answer={'found': ('object', 'Customer id as string -> name'), 'not_found': ('array', 'Ids in the given order')},
    check=check_not_found)

task('retry-metrics', R, f"""
Compute the mean of the non-null "mem" samples of every db host (hosts whose name starts with "db-").
{RETRY}
""", ['list_hosts', 'get_metrics'],
    answer={'means': ('object', 'Host -> mean mem rounded to 1 decimal')},
    expect={'means': {h: round(sum(v for _, v in metric_points(h, 'mem') if v is not None)
                               / sum(1 for _, v in metric_points(h, 'mem') if v is not None), 1)
                      for h in HOSTS if h.startswith('db-')}}, tol=0.06,
    config={'fail': {'get_metrics': {'db-02': TRANSIENT}}})


def check_backoff(t: Transcript):
    return compare({'price': QUOTES['WAYN'][0], 'attempts': 3}, t.answer() or {}, 1e-9)


task('retry-backoff', R, """
Get the WAYN quote. The quote service is rate limited: it may raise RuntimeError("429 Too Many Requests").
Retry with exponential backoff (sleep 1 s, then 2 s, then 4 s, ...) for at most 5 attempts in total.
Report the price and how many attempts were needed.
""", ['get_quote'], answer={'price': ('number', 'Price'), 'attempts': ('integer', 'Attempts including the successful one')},
    check=check_backoff,
    config={'fail': {'get_quote': {'WAYN': [['RuntimeError', '429 Too Many Requests']] * 2}}})


def check_tx(t: Transcript):
    want = {'nets': {a: r2(sum(x['amount'] for x in TRANSACTIONS[a])) for a in ('A001', 'A002', 'A004')},
            'failed': ['A003']}
    return compare(want, t.answer() or {}, 0.006)


task('retry-mixed-gather', R, f"""
Fetch the transactions of accounts A001, A002, A003 and A004 concurrently and compute each account's net
flow. {RETRY} If an account still fails after retrying, report it as failed and continue.
""", ['get_transactions'],
    answer={'nets': ('object', 'Account -> net flow rounded to 2 decimals, for accounts that worked'),
            'failed': ('array', 'Accounts that kept failing')},
    check=check_tx, config={'fail': {'get_transactions': {'A002': TRANSIENT[:1], 'A003': TRANSIENT * 4}}})

task('retry-sql-fix', R, """
Run this query: "SELECT region, COUNT(*) AS n FROM customer GROUP BY region". If it fails, inspect the
error, correct the query and run it again. Report the count per region.
""", ['query_database'],
    answer={'counts': ('object', 'Region -> number of customers')},
    expect={'counts': dict(Counter(c['region'] for c in CUSTOMERS))})


def check_page_retry(t: Transcript):
    want = sorted(i['key'] for i in ISSUES if i['priority'] == 'P1')
    return compare({'keys': want}, t.answer() or {}, 1e-9)


task('retry-search-pages', R, f"""
List every P1 issue across all pages of search_issues. {RETRY}
""", ['search_issues'], answer={'keys': ('array', 'Issue keys sorted ascending')}, check=check_page_retry,
    config={'fail': {'search_issues': {'2': TRANSIENT}}})


def check_errors_report(t: Transcript):
    ids = [1001, 1500, 1002, 2000]
    from tasks.worlds import ORDER_BY_ID
    want = {'ok': [i for i in ids if i in ORDER_BY_ID],
            'errors': {str(i): '404 Not Found: order ' + str(i) for i in ids if i not in ORDER_BY_ID}}
    return compare(want, t.answer() or {}, 1e-9)


task('retry-error-messages', R, """
Look up orders 1001, 1500, 1002 and 2000. Report which ones exist and, for those that fail, the error
message exactly as the tool raised it (str of the exception).
""", ['get_order'],
    answer={'ok': ('array', 'Existing order ids in the given order'),
            'errors': ('object', 'Failed order id as string -> error message')},
    check=check_errors_report)

flaky_hosts = ['web-01', 'web-02', 'web-03', 'web-04']
task('retry-gather-flaky', R, f"""
Fetch the cpu series of hosts {', '.join(flaky_hosts)} concurrently and report each host's max cpu
(ignore nulls). {RETRY}
""", ['get_metrics'],
    answer={'max_cpu': ('object', 'Host -> max cpu')},
    expect={'max_cpu': {h: max(v for _, v in metric_points(h, 'cpu') if v is not None) for h in flaky_hosts}},
    config={'fail': {'get_metrics': {'web-02': TRANSIENT, 'web-04': TRANSIENT[:1]}}})

