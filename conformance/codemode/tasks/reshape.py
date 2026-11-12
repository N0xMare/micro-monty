"""CSV/JSON reshaping over files, the shop and metrics."""

import csv
import io
import json
from collections import Counter, defaultdict

from ptc import Transcript, compare, r2, task
from tasks.worlds import CUSTOMERS, FILES, ORDERS, PRODUCTS, REGIONS, metric_points

C = 'csv-json-reshaping'
MONEY = 0.006


def rows(path, delimiter=','):
    return list(csv.DictReader(io.StringIO(FILES[path]), delimiter=delimiter))


def written(t: Transcript, path):
    files = [c for c in t.to('write_file') if c.get('path') == path]
    return files[-1]['content'] if files else None


def csv_check(path, want_rows):
    def check(t):
        content = written(t, path)
        if content is None:
            return f'{path} was not written'
        got = [r for r in csv.reader(io.StringIO(content)) if r]
        return compare(want_rows, got, 1e-9)
    return check


def json_check(path, want):
    def check(t):
        content = written(t, path)
        if content is None:
            return f'{path} was not written'
        try:
            got = json.loads(content)
        except ValueError as e:
            return f'{path} is not JSON: {e}'
        return compare(want, got, 1e-9)
    return check


sales = rows('/data/sales_2026q1.csv')
piv = defaultdict(lambda: {'2026-01': 0.0, '2026-02': 0.0, '2026-03': 0.0})
for r in sales:
    piv[r['rep']][r['date'][:7]] += int(r['units']) * float(r['unit_price'])
task('reshape-sales-pivot', C, """
Read /data/sales_2026q1.csv (one row per sale: date, rep, region, product, units, unit_price) and pivot
revenue (units x unit_price) by sales rep and month.
""", ['read_file'],
    answer={'pivot': ('object', 'Rep name -> {"2026-01": revenue, "2026-02": revenue, "2026-03": revenue}, every '
                      'month present (0 if none), revenues rounded to 2 decimals')},
    expect={'pivot': {k: {m: r2(v) for m, v in ms.items()} for k, ms in piv.items()}}, tol=MONEY)

emps = rows('/data/employees.csv')
by_dept = defaultdict(list)
for e in emps:
    by_dept[e['department']].append(e['name'])
task('reshape-employees-by-dept', C, """
Group the employees in /data/employees.csv by department.
""", ['read_file'],
    answer={'departments': ('object', 'Department -> list of employee names sorted alphabetically')},
    expect={'departments': {d: sorted(v) for d, v in by_dept.items()}})

eng = sorted((e for e in emps if e['department'] == 'Engineering'), key=lambda e: (-int(e['salary']), int(e['id'])))
task('reshape-engineering-json', C, """
Write /out/engineering.json: a JSON array of the Engineering employees from /data/employees.csv, each as
{"id": int, "name": str, "title": str, "salary": int}, sorted by salary descending (ties by id ascending).
""", ['read_file', 'write_file'],
    check=json_check('/out/engineering.json', [{'id': int(e['id']), 'name': e['name'], 'title': e['title'],
                                                 'salary': int(e['salary'])} for e in eng]))

inv = json.loads(FILES['/data/inventory.json'])
flat = sorted([[w['warehouse'], b['sku'], str(b['qty']), str(b['reorder_level'])]
               for w in inv['warehouses'] for b in w['bins']])
task('reshape-inventory-flatten', C, """
Flatten /data/inventory.json into a CSV file /out/inventory_flat.csv with the header
warehouse,sku,qty,reorder_level and one row per bin, sorted by warehouse then sku.
""", ['read_file', 'write_file'],
    check=csv_check('/out/inventory_flat.csv', [['warehouse', 'sku', 'qty', 'reorder_level'], *flat]))

short = sorted(({'warehouse': w['warehouse'], 'sku': b['sku'], 'short_by': b['reorder_level'] - b['qty']}
                for w in inv['warehouses'] for b in w['bins'] if b['qty'] <= b['reorder_level']),
               key=lambda x: (-x['short_by'], x['warehouse'], x['sku']))
task('reshape-reorder-list', C, """
From /data/inventory.json, list every bin whose qty is at or below its reorder_level.
""", ['read_file'],
    answer={'reorder': ('array', 'List of {"warehouse": str, "sku": str, "short_by": reorder_level - qty}, largest '
                        'short_by first, then by warehouse and sku')},
    expect={'reorder': short})

users = [json.loads(line) for line in FILES['/data/users.jsonl'].splitlines()]
roles = Counter(r for u in users if u['active'] for r in u['roles'])
task('reshape-active-roles', C, """
/data/users.jsonl has one JSON user per line. Count the active users per role (a user can have several
roles).
""", ['read_file'],
    answer={'roles': ('object', 'Role -> number of active users having it')},
    expect={'roles': dict(roles)})

task('reshape-never-logged-in', C, """
Which users in /data/users.jsonl have no last_login field at all?
""", ['read_file'],
    answer={'usernames': ('array', 'Usernames sorted ascending')},
    expect={'usernames': sorted(u['username'] for u in users if 'last_login' not in u)})

bank = rows('/data/bank_feb.csv', ';')
by_purpose = defaultdict(float)
for b in bank:
    by_purpose[b['Verwendungszweck']] += float(b['Betrag'].replace(',', '.'))
debit = min(bank, key=lambda b: float(b['Betrag'].replace(',', '.')))
d, m, y = debit['Datum'].split('.')
task('reshape-bank-eu-csv', C, """
/data/bank_feb.csv is a German bank export: semicolon-separated, dates as DD.MM.YYYY, amounts with a
decimal comma (negative = debit). Total the amounts per Verwendungszweck (purpose), and find the date of
the single largest debit.
""", ['read_file'],
    answer={'totals': ('object', 'Purpose -> summed amount as a number, rounded to 2 decimals'),
            'largest_debit_date': 'Date of the largest debit as YYYY-MM-DD'},
    expect={'totals': {k: r2(v) for k, v in by_purpose.items()}, 'largest_debit_date': f'{y}-{m}-{d}'}, tol=MONEY)

survey = rows('/data/survey.csv')
task('reshape-survey', C, """
Summarize /data/survey.csv (columns respondent, score, comment; comments may contain commas, quotes and
line breaks).
""", ['read_file'],
    answer={'responses': ('integer', 'Number of respondents'),
            'average_score': ('number', 'Mean score rounded to 2 decimals'),
            'with_comment': ('integer', 'Respondents with a non-empty comment'),
            'multiline': ('array', 'Respondent ids whose comment spans more than one line, ascending')},
    expect={'responses': len(survey), 'average_score': r2(sum(int(s['score']) for s in survey) / len(survey)),
            'with_comment': sum(bool(s['comment']) for s in survey),
            'multiline': sorted(s['respondent'] for s in survey if '\n' in s['comment'])}, tol=MONEY)

names = {e['id']: e['name'] for e in emps}
task('reshape-manager-join', C, """
In /data/employees.csv, manager_id refers to another employee's id (empty for top-level managers). For
every employee who has a manager, report the manager's name.
""", ['read_file'],
    answer={'reports': ('array', 'List of {"id": int, "name": str, "manager": manager name}, by id ascending')},
    expect={'reports': [{'id': int(e['id']), 'name': e['name'], 'manager': names[e['manager_id']]}
                        for e in emps if e['manager_id']]})


def parse_conf(text):
    out, cur = {}, None
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith('#'):
            continue
        if line.startswith('['):
            cur = out.setdefault(line[1:-1], {})
            continue
        k, v = (x.strip() for x in line.split('=', 1))
        v = int(v) if v.isdigit() else {'true': True, 'false': False}.get(v, v)
        (cur if cur is not None else out)[k] = v
    return out


task('reshape-parse-config', C, """
Parse /etc/app.conf (an INI-style file: "key = value" lines, "[section]" headers, "#" comments) into a
nested object: keys before the first section at the top level, each section as a nested object.
Convert values that are all digits to integers and "true"/"false" to booleans; keep everything else as
a string with surrounding spaces removed.
""", ['read_file'],
    answer={'config': ('object', 'The parsed configuration')},
    expect={'config': parse_conf(FILES['/etc/app.conf'])})

task('reshape-toml-settings', C, """
Read /etc/settings.toml and report the server port, the number of workers, and the names of the
features that are enabled.
""", ['read_file'],
    answer={'port': ('integer', 'server.port'), 'workers': ('integer', 'server.workers'),
            'enabled_features': ('array', 'Names of [[features]] entries with enabled = true, in file order')},
    expect={'port': 8443, 'workers': 4, 'enabled_features': ['invoices']})

c5 = sorted((o for o in ORDERS if o['customer_id'] == 5), key=lambda o: (o['order_date'], o['id']))
task('reshape-orders-to-csv', C, """
Export all orders of customer 5 to /out/customer5.csv with the header id,date,status,total, oldest first
(ties by id), total formatted with exactly two decimals (e.g. 12.50).
""", ['list_orders', 'write_file'],
    check=csv_check('/out/customer5.csv', [['id', 'date', 'status', 'total'],
                                           *[[str(o['id']), o['order_date'], o['status'], f'{o["total"]:.2f}'] for o in c5]]))

nest = defaultdict(Counter)
for c in CUSTOMERS:
    nest[c['region']][c['tier']] += 1
task('reshape-nested-counts', C, """
Build a nested count of customers: region -> tier -> number of customers. Only include tiers that occur
in a region.
""", ['list_customers'],
    answer={'counts': ('object', 'Region -> {tier: count}')},
    expect={'counts': {r: dict(nest[r]) for r in REGIONS if r in nest}})

cpu, mem = metric_points('web-01', 'cpu'), metric_points('web-01', 'mem')


def hhmm(ts):
    return f'{ts % 86400 // 3600:02d}:{ts % 3600 // 60:02d}'


task('reshape-merge-series', C, """
Merge the cpu and mem series of host web-01 into one table of the first 8 samples (00:00 to 01:45 UTC).
""", ['get_metrics'],
    answer={'rows': ('array', 'List of {"time": "HH:MM" UTC, "cpu": value or null, "mem": value or null}, in time order')},
    expect={'rows': [{'time': hhmm(a[0]), 'cpu': a[1], 'mem': b[1]} for a, b in list(zip(cpu, mem))[:8]]})

pid = {p['name']: p['id'] for p in PRODUCTS}
units = Counter()
for r in sales:
    units[pid[r['product']]] += int(r['units'])
task('reshape-names-to-skus', C, """
/data/sales_2026q1.csv identifies products by name. Map each name to its product id using the product
catalogue, then total the units sold per product id.
""", ['read_file', 'list_products'],
    answer={'units': ('object', 'Product id -> units sold')},
    expect={'units': dict(units)})

users_rows = [['id', 'username', 'email', 'roles', 'active', 'last_login']] + [
    [str(u['id']), u['username'], u['email'], ';'.join(u['roles']), str(u['active']).lower(), u.get('last_login', '')]
    for u in sorted(users, key=lambda u: u['id'])]
task('reshape-jsonl-to-csv', C, """
Convert /data/users.jsonl to /out/users.csv with the header id,username,email,roles,active,last_login,
sorted by id. Join roles with ";" in their original order, write active as true/false (lowercase), and
leave last_login empty when it is missing.
""", ['read_file', 'write_file'],
    check=csv_check('/out/users.csv', users_rows))
