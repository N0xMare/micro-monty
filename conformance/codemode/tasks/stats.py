"""Numeric summaries and statistics."""

import csv
import io
import math
import statistics
from collections import defaultdict

from ptc import r2, task
from tasks.worlds import (ACCOUNTS, FILES, HOSTS, LOG_DATES, LOGS, ORDERS, PRODUCTS, QUOTES, TRANSACTIONS,
                          metric_points, parse_log_line)

C = 'numeric-statistics'
done = [o['total'] for o in ORDERS if o['status'] == 'completed']


def vals(h, m):
    return [v for _, v in metric_points(h, m) if v is not None]


def nearest_rank(xs, p):
    xs = sorted(xs)
    return xs[max(1, math.ceil(p / 100 * len(xs))) - 1]


task('stat-order-summary', C, """
Summarize the totals of all completed orders: count, mean, median, sample standard deviation, min and max.
""", ['query_database'],
    answer={k: ('number', 'Rounded to 2 decimals') for k in ('mean', 'median', 'stdev', 'min', 'max')}
    | {'count': ('integer', 'Number of completed orders')},
    expect={'count': len(done), 'mean': r2(statistics.mean(done)), 'median': r2(statistics.median(done)),
            'stdev': r2(statistics.stdev(done)), 'min': min(done), 'max': max(done)}, tol=0.006)

lat = vals('web-03', 'latency_ms')
task('stat-latency-percentiles', C, """
Compute p50, p90 and p99 of host web-03's latency_ms series (ignore nulls) with the nearest-rank method:
the p-th percentile is the value at 1-based position ceil(p/100 * n) of the ascending sorted values.
""", ['get_metrics'],
    answer={'p50': ('number', ''), 'p90': ('number', ''), 'p99': ('number', '')},
    expect={'p50': nearest_rank(lat, 50), 'p90': nearest_rank(lat, 90), 'p99': nearest_rank(lat, 99)})

cpu = metric_points('web-05', 'cpu')
cv = [v for _, v in cpu if v is not None]
mu, sd = statistics.mean(cv), statistics.pstdev(cv)
anom = [t for t, v in cpu if v is not None and abs(v - mu) > 2 * sd]
task('stat-zscore-anomalies', C, """
Find anomalies in host web-05's cpu series: samples more than 2 population standard deviations away from
the mean of the non-null samples.
""", ['get_metrics'],
    answer={'timestamps': ('array', 'Epoch seconds of anomalous samples, in time order')},
    expect={'timestamps': anom})

mem = [v for _, v in metric_points('db-01', 'mem')]
ma = []
for i in range(len(mem) - 3):
    w = mem[i:i + 4]
    ma.append(None if None in w else round(sum(w) / 4, 2))
task('stat-moving-average', C, """
Compute the 4-sample moving average of host db-01's mem series: for each window of 4 consecutive samples
(windows starting at sample 0, 1, 2, ...), the mean rounded to 2 decimals, or null if the window contains a
null.
""", ['get_metrics'],
    answer={'averages': ('array', 'One value per window, in order')},
    expect={'averages': ma}, tol=0.006)

a, b = [], []
for (t1, x), (t2, y) in zip(metric_points('web-01', 'cpu'), metric_points('web-01', 'latency_ms')):
    if x is not None and y is not None:
        a.append(x)
        b.append(y)
task('stat-correlation', C, """
Compute the Pearson correlation between host web-01's cpu and latency_ms series, using only timestamps
where both values are present.
""", ['get_metrics'],
    answer={'r': ('number', 'Correlation rounded to 3 decimals'), 'n': ('integer', 'Number of paired samples')},
    expect={'r': round(statistics.correlation(a, b), 3), 'n': len(a)}, tol=0.0011)

hist = {f'{lo}-{lo + 499}': sum(lo <= x < lo + 500 for x in done) for lo in range(0, 3000, 500)}
hist['3000+'] = sum(x >= 3000 for x in done)
task('stat-histogram', C, """
Bucket the totals of completed orders into a histogram with buckets 0-499, 500-999, ..., 2500-2999 and
3000+ (a total t falls in bucket lo-hi when lo <= t < hi+1).
""", ['query_database'],
    answer={'buckets': ('object', 'Bucket label exactly as listed ("0-499", ..., "3000+") -> count, every bucket')},
    expect={'buckets': hist})

emps = list(csv.DictReader(io.StringIO(FILES['/data/employees.csv'])))
sal = defaultdict(list)
for e in emps:
    sal[e['department']].append(int(e['salary']))
task('stat-salary-by-dept', C, """
From /data/employees.csv compute the median and mean salary of each department.
""", ['read_file'],
    answer={'departments': ('object', 'Department -> {"median": number, "mean": number rounded to 2 decimals}')},
    expect={'departments': {d: {'median': statistics.median(v), 'mean': r2(statistics.mean(v))} for d, v in sal.items()}},
    tol=0.006)

allv = {h: vals(h, 'cpu') for h in HOSTS}
task('stat-host-cv', C, """
For every host, compute the coefficient of variation of its cpu series (sample standard deviation divided
by mean, non-null samples) and report the most and least variable hosts.
""", ['list_hosts', 'get_metrics'],
    answer={'most_variable': 'Host', 'least_variable': 'Host'},
    expect={'most_variable': max(HOSTS, key=lambda h: statistics.stdev(allv[h]) / statistics.mean(allv[h])),
            'least_variable': min(HOSTS, key=lambda h: statistics.stdev(allv[h]) / statistics.mean(allv[h]))})

errs = vals('web-06', 'errors')
task('stat-error-rate', C, """
Host web-06's "errors" series counts errors per 15-minute sample. What is the mean number of errors per
hour (over non-null samples, 4 samples per hour), and what fraction of samples had zero errors?
""", ['get_metrics'],
    answer={'per_hour': ('number', 'Rounded to 2 decimals'), 'zero_fraction': ('number', 'Rounded to 3 decimals')},
    expect={'per_hour': r2(statistics.mean(errs) * 4), 'zero_fraction': round(sum(e == 0 for e in errs) / len(errs), 3)},
    tol=0.006)

lat_all = []
for d in LOG_DATES:
    for line in LOGS[('search', d)]:
        e = parse_log_line(line)
        if e[3].startswith('request completed'):
            lat_all.append(int(e[3].split('latency_ms=')[1].split()[0]))
task('stat-log-latency', C, """
From the search service's logs for all three days, collect latency_ms of every completed request and
report the mean, median, and the p95 by the nearest-rank method (value at 1-based position ceil(0.95*n)
of the sorted values).
""", ['fetch_logs'],
    answer={'mean': ('number', 'Rounded to 1 decimal'), 'median': ('number', ''), 'p95': ('number', '')},
    expect={'mean': round(statistics.mean(lat_all), 1), 'median': statistics.median(lat_all),
            'p95': nearest_rank(lat_all, 95)}, tol=0.06)

prices = [p['price'] for p in PRODUCTS]
q = statistics.quantiles(prices, n=4, method='inclusive')
task('stat-price-quartiles', C, """
Compute the quartiles (Q1, Q2, Q3) of all product prices using linear interpolation between closest ranks
(the "inclusive" definition: position (n-1)*k/4 in the 0-based sorted list, interpolated).
""", ['list_products'],
    answer={'q1': ('number', 'Rounded to 2 decimals'), 'q2': ('number', ''), 'q3': ('number', '')},
    expect={'q1': r2(q[0]), 'q2': r2(q[1]), 'q3': r2(q[2])}, tol=0.006)

w = {s: QUOTES[s][0] for s in ('ACME', 'INIT', 'STRK')}
task('stat-weighted-change', C, """
Compute the price-weighted average daily change (sum of price * change_pct divided by sum of prices) of
ACME, INIT and STRK.
""", ['get_quote'],
    answer={'weighted_change_pct': ('number', 'Rounded to 3 decimals')},
    expect={'weighted_change_pct': round(sum(QUOTES[s][0] * QUOTES[s][1] for s in w) / sum(w.values()), 3)},
    tol=0.0011)

nets = []
for acc in TRANSACTIONS:
    run, low = 0.0, 0.0
    for t in TRANSACTIONS[acc]:
        run += t['amount']
        low = min(low, run)
    nets.append((acc, round(low, 2)))
task('stat-max-drawdown', C, """
For accounts A001 to A012, walk each account's transactions in order keeping a running sum that starts at
0, and record the lowest value the running sum reaches (0 if it never goes negative). Report the account
with the deepest low and the value.
""", ['get_transactions'],
    answer={'account_id': 'Account id', 'low': ('number', 'Rounded to 2 decimals')},
    expect={'account_id': min(nets, key=lambda x: x[1])[0], 'low': min(x[1] for x in nets)}, tol=0.006)

bal = [a['balance'] for a in ACCOUNTS if a['status'] == 'active']
task('stat-balance-summary', C, """
Over all active accounts (all pages of list_accounts), report the number of accounts, the sum of balances
(ignore currencies), the geometric mean of the positive balances, and the harmonic mean of the positive
balances.
""", ['list_accounts'],
    answer={'count': ('integer', ''), 'sum': ('number', 'Rounded to 2 decimals'),
            'geometric_mean': ('number', 'Rounded to 2 decimals'), 'harmonic_mean': ('number', 'Rounded to 2 decimals')},
    expect={'count': len(bal), 'sum': r2(sum(bal)), 'geometric_mean': r2(statistics.geometric_mean([x for x in bal if x > 0])),
            'harmonic_mean': r2(statistics.harmonic_mean([x for x in bal if x > 0]))}, tol=0.02)

xs = list(range(len(vals('db-02', 'latency_ms'))))
pts = [(i, v) for i, (_, v) in enumerate(metric_points('db-02', 'latency_ms')) if v is not None]
slope, icpt = statistics.linear_regression([p[0] for p in pts], [p[1] for p in pts])
task('stat-trend', C, """
Fit a least-squares line to host db-02's latency_ms series, with x = the sample index (0 for the first
sample, counting nulls too) and y = the value, skipping null samples. Report the slope and intercept and
the predicted value at x = 48.
""", ['get_metrics'],
    answer={'slope': ('number', 'Rounded to 4 decimals'), 'intercept': ('number', 'Rounded to 2 decimals'),
            'predicted_48': ('number', 'Rounded to 2 decimals')},
    expect={'slope': round(slope, 4), 'intercept': r2(icpt), 'predicted_48': r2(slope * 48 + icpt)}, tol=0.006)

stock = sorted(p['stock'] for p in PRODUCTS)
task('stat-stock-mode', C, """
Across all products, what is the most common stock level (ties: the smallest value), how many products
are out of stock, and what share of products have at least 20 units?
""", ['list_products'],
    answer={'mode': ('integer', ''), 'out_of_stock': ('integer', ''), 'share_20_plus': ('number', 'Percentage rounded to 1 decimal')},
    expect={'mode': min(statistics.multimode(stock)), 'out_of_stock': stock.count(0),
            'share_20_plus': round(sum(s >= 20 for s in stock) / len(stock) * 100, 1)}, tol=0.06)

mm = vals('web-07', 'mem')
lo, hi = min(mm), max(mm)
task('stat-normalize', C, """
Min-max normalize host web-07's mem series (non-null samples, in order) to the range 0..1 and report the
normalized values rounded to 3 decimals, plus the original min and max.
""", ['get_metrics'],
    answer={'normalized': ('array', 'Values in order'), 'min': ('number', ''), 'max': ('number', '')},
    expect={'normalized': [round((v - lo) / (hi - lo), 3) for v in mm], 'min': lo, 'max': hi}, tol=0.0011)
