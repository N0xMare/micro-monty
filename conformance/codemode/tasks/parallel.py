"""Parallel calls with asyncio.gather (fan-out and reduce)."""

import statistics
from collections import Counter

from ptc import Transcript, compare, r2, task
from tasks.worlds import (ACCOUNTS, CITIES, CUSTOMER_BY_ID, ENDPOINTS, HOSTS, LOG_DATES, LOGS, ORDER_BY_ID,
                          ORDER_ITEMS, PRODUCT_BY_ID, PRODUCTS, QUOTES, SERVICES, TRANSACTIONS,
                          metric_points, parse_log_line)

C = 'parallel-gather'
PAR = 'Independent lookups should run concurrently.'

cities = ['London', 'Paris', 'Berlin', 'Madrid', 'Rome', 'Oslo', 'Tokyo', 'Sydney']
warm = max(cities, key=lambda c: CITIES[c][0])
task('par-weather-warmest', C, f"""
Get the current weather for {', '.join(cities)}. {PAR} Report the warmest and coldest city and the mean
temperature.
""", ['get_weather'],
    answer={'warmest': 'City', 'coldest': 'City', 'mean_temp_c': ('number', 'Rounded to 1 decimal')},
    expect={'warmest': warm, 'coldest': min(cities, key=lambda c: CITIES[c][0]),
            'mean_temp_c': round(sum(CITIES[c][0] for c in cities) / len(cities), 1)}, tol=0.06)

syms = sorted(QUOTES)
task('par-portfolio-value', C, """
A portfolio holds ACME 120, GLBX 300, INIT 15, UMBR 1000 and WAYN 40 shares. Fetch the quotes concurrently
and compute the portfolio's total value and the position with the largest value.
""", ['get_quote'],
    answer={'total_usd': ('number', 'Rounded to 2 decimals'), 'largest': 'Symbol of the largest position'},
    expect={'total_usd': r2(sum(QUOTES[s][0] * n for s, n in
                                 (('ACME', 120), ('GLBX', 300), ('INIT', 15), ('UMBR', 1000), ('WAYN', 40)))),
            'largest': max((('ACME', 120), ('GLBX', 300), ('INIT', 15), ('UMBR', 1000), ('WAYN', 40)),
                           key=lambda x: QUOTES[x[0]][0] * x[1])[0]}, tol=0.006)

movers = sorted(syms, key=lambda s: (-abs(QUOTES[s][1]), s))[:3]
task('par-top-movers', C, f"""
Fetch quotes for {', '.join(syms)} concurrently and report the three symbols with the largest absolute
percentage change (ties by symbol).
""", ['get_quote'],
    answer={'movers': ('array', 'List of {"symbol": str, "change_pct": float}, largest absolute change first')},
    expect={'movers': [{'symbol': s, 'change_pct': QUOTES[s][1]} for s in movers]})

eps = sorted(ENDPOINTS)
task('par-health-sweep', C, f"""
Probe every one of these endpoints concurrently: {', '.join(eps)}. Report which are down and the average
latency of those that are up.
""", ['check_health'],
    answer={'down': ('array', 'URLs that are down, sorted'), 'avg_latency_ms': ('number', 'Rounded to 1 decimal')},
    expect={'down': sorted(u for u in eps if not ENDPOINTS[u][0]),
            'avg_latency_ms': round(statistics.mean(v[1] for v in ENDPOINTS.values() if v[0]), 1)}, tol=0.06)


def cpu_mean(h):
    vals = [v for _, v in metric_points(h, 'cpu') if v is not None]
    return sum(vals) / len(vals)


hot = sorted(HOSTS, key=lambda h: -cpu_mean(h))[:3]
task('par-hottest-hosts', C, f"""
List the cluster's hosts, fetch each host's cpu series concurrently, and report the three hosts with the
highest mean cpu (ignore null samples). {PAR}
""", ['list_hosts', 'get_metrics'],
    answer={'hosts': ('array', 'List of {"host": str, "mean_cpu": number rounded to 1 decimal}, highest first')},
    expect={'hosts': [{'host': h, 'mean_cpu': round(cpu_mean(h), 1)} for h in hot]}, tol=0.06)

errs = {}
for s in SERVICES:
    errs[s] = sum(parse_log_line(line)[1] == 'ERROR' for d in LOG_DATES for line in LOGS[(s, d)])
task('par-errors-all-days', C, """
Fetch every service's logs for all three days (2026-03-13, 2026-03-14, 2026-03-15) concurrently and count
ERROR lines per service over the three days.
""", ['list_services', 'fetch_logs'],
    answer={'errors': ('object', 'Service -> ERROR line count')},
    expect={'errors': errs})

oids = [1003, 1017, 1029, 1044, 1058, 1071, 1090, 1123, 1150, 1199]
task('par-order-customers', C, f"""
For orders {', '.join(map(str, oids))}: fetch each order and then each distinct customer, concurrently
where possible. Report each order's customer name.
""", ['get_order', 'get_customer'],
    answer={'customers': ('object', 'Order id as a string -> customer name')},
    expect={'customers': {str(o): CUSTOMER_BY_ID[ORDER_BY_ID[o]['customer_id']]['name'] for o in oids}})

accts = [a['account_id'] for a in ACCOUNTS[:12]]
net = {a: r2(sum(t['amount'] for t in TRANSACTIONS[a])) for a in accts}
task('par-account-net-flow', C, f"""
Fetch the transactions of accounts {', '.join(accts)} concurrently and compute each account's net flow
(sum of amounts). Report the accounts with a negative net flow.
""", ['get_transactions'],
    answer={'negative': ('object', 'Account id -> net flow rounded to 2 decimals, only negative ones')},
    expect={'negative': {a: v for a, v in net.items() if v < 0}}, tol=0.006)

partial = ['London', 'Atlantis', 'Tokyo', 'Gotham', 'Austin', 'Chicago']


def check_partial(t: Transcript):
    return compare({'temps': {c: CITIES[c][0] for c in partial if c in CITIES}, 'failed': ['Atlantis', 'Gotham']},
                   t.answer() or {}, 1e-9, ('failed',))


task('par-partial-failure', C, f"""
Get the temperature for {', '.join(partial)} concurrently. Some cities may not exist (the tool raises an
error for them); one failure must not lose the other results.
""", ['get_weather'],
    answer={'temps': ('object', 'City -> temp_c for cities that worked'),
            'failed': ('array', 'Cities whose lookup failed, in the order given')},
    check=check_partial)

pids = [p['id'] for p in PRODUCTS[:12]]
value = {p: PRODUCT_BY_ID[p]['price'] * PRODUCT_BY_ID[p]['stock'] for p in pids}
task('par-stock-value', C, f"""
Fetch products {', '.join(pids)} concurrently and compute the inventory value of each (price x stock).
Report the total and the most valuable product.
""", ['get_product'],
    answer={'total': ('number', 'Rounded to 2 decimals'), 'top_product': 'Product id'},
    expect={'total': r2(sum(value.values())), 'top_product': max(value, key=value.get)}, tol=0.006)

pages = [f'https://shop.example.com/products?id={i}' for i in range(1, 13)]
task('par-fetch-pages', C, """
Fetch https://shop.example.com/products?id=1 through id=12 concurrently. Each page has a
<div class="stock">N in stock</div>. Report the page ids whose stock is 0, and the total stock.
""", ['http_get'],
    answer={'out_of_stock': ('array', 'Page ids (int), ascending'), 'total_stock': ('integer', 'Sum of stock')},
    expect={'out_of_stock': [i for i in range(1, 13) if PRODUCTS[i - 1]['stock'] == 0],
            'total_stock': sum(PRODUCTS[i - 1]['stock'] for i in range(1, 13))})

err_hosts = {}
for h in HOSTS:
    e = sum(v for _, v in metric_points(h, 'errors') if v is not None)
    err_hosts[h] = e
task('par-many-calls', C, """
For every host in the cluster fetch both the "errors" and the "latency_ms" series, all concurrently in one
batch, and report per host the total errors and the max latency (ignore nulls).
""", ['list_hosts', 'get_metrics'],
    answer={'hosts': ('object', 'Host -> {"errors": int, "max_latency_ms": float}')},
    expect={'hosts': {h: {'errors': err_hosts[h],
                          'max_latency_ms': max(v for _, v in metric_points(h, 'latency_ms') if v is not None)}
                      for h in HOSTS}})

bad_orders = [1012, 999, 1101, 5000, 1222]


def check_orders(t):
    ok = [o for o in bad_orders if o in ORDER_BY_ID]
    return compare({'found': {str(o): ORDER_BY_ID[o]['total'] for o in ok},
                    'missing': [o for o in bad_orders if o not in ORDER_BY_ID]}, t.answer() or {}, 1e-6)


task('par-orders-some-missing', C, f"""
Look up orders {', '.join(map(str, bad_orders))} concurrently. Some ids do not exist (the tool raises a
not-found error). Report the totals of those found and the ids that are missing.
""", ['get_order'],
    answer={'found': ('object', 'Order id as string -> total'), 'missing': ('array', 'Missing ids in the order given')},
    check=check_orders)

cats = ['hardware', 'software', 'accessories', 'services']
task('par-category-prices', C, """
For each product category (hardware, software, accessories, services), list its products; run the four
listings concurrently. Report the median price per category.
""", ['list_products'],
    answer={'medians': ('object', 'Category -> median price rounded to 2 decimals')},
    expect={'medians': {c: r2(statistics.median(p['price'] for p in PRODUCTS if p['category'] == c)) for c in cats}},
    tol=0.006)

pairs = [('USD', 'EUR'), ('USD', 'GBP'), ('USD', 'CAD'), ('USD', 'JPY'), ('USD', 'CHF'), ('USD', 'AUD')]
task('par-fx-table', C, """
Fetch the exchange rates from USD to EUR, GBP, CAD, JPY, CHF and AUD concurrently and convert 2,500 USD
into each.
""", ['get_exchange_rate'],
    answer={'converted': ('object', 'Currency -> converted amount rounded to 2 decimals')},
    expect={'converted': {q: r2(2500 * r) for (_, q), r in zip(pairs, [0.9213, 0.7841, 1.3612, 148.27, 0.8822, 1.5234])}},
    tol=0.006)

cond = Counter(CITIES[c][1] for c in CITIES)
task('par-weather-conditions', C, f"""
Get the weather for all of these cities concurrently: {', '.join(CITIES)}. Count the cities per condition.
""", ['get_weather'],
    answer={'conditions': ('object', 'Condition -> number of cities')},
    expect={'conditions': dict(cond)})

items_1100 = [i for i in ORDER_ITEMS if 1100 <= i['order_id'] < 1110]
units_1100 = Counter()
for i in items_1100:
    units_1100[PRODUCT_BY_ID[i['product_id']]['category']] += i['qty']
task('par-two-level-fanout', C, """
Fetch orders 1100 through 1109 concurrently, then fetch every distinct product that appears in their line
items concurrently. Report the total quantity sold per product category over those ten orders.
""", ['get_order', 'get_product'],
    answer={'units': ('object', 'Category -> total quantity')},
    expect={'units': dict(units_1100)})
