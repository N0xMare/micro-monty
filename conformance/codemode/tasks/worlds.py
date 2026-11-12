"""Fixture worlds and their mocked tools.

Every dataset is built from fixed seeds, so each run sees the same data.
Tool implementations take `(args, state)`; `state['config']` is the task's
config (failure injection, overrides) and `state` keeps per-run state.
"""

from __future__ import annotations

import base64
import datetime as dt
import hashlib
import json
import math
import random
import sqlite3
import urllib.parse

from ptc import ToolError, tool

# ==========================================================================
# Shop: customers, products, orders (SQL and REST)

REGIONS = ['North', 'South', 'East', 'West']
FIRST = ['Ada', 'Ben', 'Cara', 'Dev', 'Eli', 'Fay', 'Gus', 'Hana', 'Ivan', 'Jade', 'Kai', 'Lena',
         'Milo', 'Nina', 'Omar', 'Pia', 'Quinn', 'Rosa', 'Sam', 'Tara']
LAST = ['Smith', 'Okafor', 'Novak', 'Garcia', 'Chen', 'Haddad', 'Larsen', 'Moreau', 'Silva', 'Kim']
CATEGORIES = ['hardware', 'software', 'accessories', 'services']
PRODUCT_NAMES = {
    'hardware': ['Laptop Pro 14', 'Laptop Air 13', 'Desktop Tower', 'Monitor 27', 'Monitor 32', 'Dock Station'],
    'software': ['Office Suite', 'Antivirus Plus', 'Design Studio', 'Backup Cloud', 'VPN Annual', 'Dev Tools'],
    'accessories': ['Wireless Mouse', 'Mech Keyboard', 'USB-C Hub', 'Webcam HD', 'Headset', 'Laptop Sleeve'],
    'services': ['Setup Service', 'Extended Warranty', 'Data Recovery', 'Training Session', 'Onsite Repair', 'Consulting Hour'],
}


def _shop():
    rnd = random.Random(20260315)
    customers = []
    for i in range(1, 41):
        first, last = FIRST[(i * 7) % 20], LAST[(i * 3) % 10]
        customers.append({
            'id': i, 'name': f'{first} {last}', 'email': f'{first.lower()}.{last.lower()}{i}@example.com',
            'region': REGIONS[rnd.randrange(4)], 'tier': rnd.choice(['bronze', 'bronze', 'silver', 'gold']),
            'signup_date': (dt.date(2024, 1, 1) + dt.timedelta(days=rnd.randrange(700))).isoformat(),
            'country': rnd.choice(['US', 'US', 'US', 'CA', 'GB', 'DE']),
        })
    products = []
    n = 1
    for cat in CATEGORIES:
        for name in PRODUCT_NAMES[cat]:
            base = {'hardware': 600, 'software': 90, 'accessories': 35, 'services': 120}[cat]
            price = round(base * (0.5 + rnd.random()) + 0.99, 2)
            products.append({'id': f'P{n:02d}', 'name': name, 'category': cat, 'price': price,
                             'stock': rnd.randrange(0, 80)})
            n += 1
    orders, items = [], []
    start = dt.date(2025, 10, 1)
    for k in range(300):
        oid = 1001 + k
        cust = customers[rnd.randrange(40)]
        day = start + dt.timedelta(days=rnd.randrange(165))
        status = rnd.choices(['completed', 'pending', 'cancelled', 'refunded'], [70, 12, 10, 8])[0]
        total = 0.0
        for _ in range(rnd.choice([1, 1, 2, 3])):
            p = products[rnd.randrange(len(products))]
            qty = rnd.choice([1, 1, 1, 2, 3, 5])
            items.append({'order_id': oid, 'product_id': p['id'], 'qty': qty, 'unit_price': p['price']})
            total += qty * p['price']
        orders.append({'id': oid, 'customer_id': cust['id'], 'order_date': day.isoformat(),
                       'status': status, 'total': round(total, 2), 'currency': 'USD'})
    return customers, products, orders, items


CUSTOMERS, PRODUCTS, ORDERS, ORDER_ITEMS = _shop()
CUSTOMER_BY_ID = {c['id']: c for c in CUSTOMERS}
PRODUCT_BY_ID = {p['id']: p for p in PRODUCTS}
ORDER_BY_ID = {o['id']: o for o in ORDERS}


def _db() -> sqlite3.Connection:
    db = sqlite3.connect(':memory:', check_same_thread=False)
    db.execute('create table customers (id integer primary key, name text, email text, region text, '
               'tier text, signup_date text, country text)')
    db.execute('create table products (id text primary key, name text, category text, price real, stock integer)')
    db.execute('create table orders (id integer primary key, customer_id integer, order_date text, '
               'status text, total real, currency text)')
    db.execute('create table order_items (order_id integer, product_id text, qty integer, unit_price real)')
    for table, rows in (('customers', CUSTOMERS), ('products', PRODUCTS), ('orders', ORDERS),
                        ('order_items', ORDER_ITEMS)):
        cols = list(rows[0])
        db.executemany(f'insert into {table} ({",".join(cols)}) values ({",".join("?" * len(cols))})',
                       [tuple(r[c] for c in cols) for r in rows])
    return db


DB = _db()

SCHEMA_DOC = (
    'Tables: customers(id INTEGER, name TEXT, email TEXT, region TEXT, tier TEXT, signup_date TEXT '
    'YYYY-MM-DD, country TEXT); products(id TEXT like "P01", name TEXT, category TEXT, price REAL, '
    'stock INTEGER); orders(id INTEGER, customer_id INTEGER, order_date TEXT YYYY-MM-DD, status TEXT one of '
    'completed/pending/cancelled/refunded, total REAL, currency TEXT); order_items(order_id INTEGER, '
    'product_id TEXT, qty INTEGER, unit_price REAL). SQLite dialect.'
)


@tool('query_database', 'Run a read-only SQL query against the shop database. ' + SCHEMA_DOC,
      {'sql': ('string', 'The SQL query')},
      'a list of rows, each an object mapping column name to value')
def _query_database(args, state):
    sql = args['sql']
    if not sql.lstrip().lower().startswith(('select', 'with')):
        raise ToolError('only SELECT queries are allowed', 'PermissionError')
    try:
        cur = DB.execute(sql)
    except sqlite3.Error as e:
        raise ToolError(f'SQL error: {e}', 'ValueError') from None
    cols = [d[0] for d in cur.description]
    return [dict(zip(cols, row)) for row in cur.fetchall()]


def _not_found(what):
    return ToolError(f'404 Not Found: {what}', 'LookupError')


@tool('get_customer', 'Fetch one customer by id.',
      {'customer_id': ('integer', 'Customer id')},
      'an object {"id": int, "name": str, "email": str, "region": str, "tier": "bronze"|"silver"|"gold", '
      '"signup_date": "YYYY-MM-DD", "country": str}')
def _get_customer(args, state):
    c = CUSTOMER_BY_ID.get(args['customer_id'])
    if c is None:
        raise _not_found(f'customer {args["customer_id"]}')
    return c


@tool('list_customers', 'List customers, 10 per page, ordered by id. Optional filters by region and tier.',
      {'region': ('string', 'Optional region: North, South, East or West'),
       'tier': ('string', 'Optional tier: bronze, silver or gold'),
       'page': ('integer', 'Page number, starting at 1 (default 1)')},
      'an object {"customers": [customer objects with id, name, email, region, tier, signup_date, country], '
      '"page": int, "total_pages": int}', required=[])
def _list_customers(args, state):
    rows = [c for c in CUSTOMERS if args.get('region') in (None, c['region'])
            and args.get('tier') in (None, c['tier'])]
    page = args.get('page', 1)
    pages = max(1, math.ceil(len(rows) / 10))
    return {'customers': rows[(page - 1) * 10:page * 10], 'page': page, 'total_pages': pages}


def _order_summary(o):
    return dict(o)


@tool('list_orders', 'List orders, newest first, 25 per page. Optional filters.',
      {'customer_id': ('integer', 'Only orders of this customer'),
       'status': ('string', 'Only orders with this status: completed, pending, cancelled or refunded'),
       'since': ('string', 'Only orders on or after this date, YYYY-MM-DD'),
       'until': ('string', 'Only orders on or before this date, YYYY-MM-DD'),
       'cursor': ('string', 'Cursor from a previous page\'s next_cursor')},
      'an object {"orders": [{"id": int, "customer_id": int, "order_date": "YYYY-MM-DD", "status": str, '
      '"total": float, "currency": "USD"}], "next_cursor": str or null (null on the last page)}', required=[])
def _list_orders(args, state):
    rows = [o for o in ORDERS
            if args.get('customer_id') in (None, o['customer_id'])
            and args.get('status') in (None, o['status'])
            and o['order_date'] >= args.get('since', '0000')
            and o['order_date'] <= args.get('until', '9999')]
    rows.sort(key=lambda o: (o['order_date'], o['id']), reverse=True)
    start = 0
    if args.get('cursor'):
        try:
            start = int(base64.b64decode(args['cursor']).decode().split(':')[1])
        except Exception:
            raise ToolError('invalid cursor', 'ValueError') from None
    page = rows[start:start + 25]
    nxt = base64.b64encode(f'offset:{start + 25}'.encode()).decode() if start + 25 < len(rows) else None
    return {'orders': [_order_summary(o) for o in page], 'next_cursor': nxt}


@tool('get_order', 'Fetch one order with its line items.',
      {'order_id': ('integer', 'Order id')},
      'an object {"id": int, "customer_id": int, "order_date": str, "status": str, "total": float, '
      '"currency": str, "items": [{"product_id": str, "qty": int, "unit_price": float}]}')
def _get_order(args, state):
    o = ORDER_BY_ID.get(args['order_id'])
    if o is None:
        raise _not_found(f'order {args["order_id"]}')
    its = [{k: i[k] for k in ('product_id', 'qty', 'unit_price')} for i in ORDER_ITEMS if i['order_id'] == o['id']]
    return dict(o, items=its)


@tool('get_product', 'Fetch one product.',
      {'product_id': ('string', 'Product id like "P07"')},
      'an object {"id": str, "name": str, "category": str, "price": float, "stock": int}')
def _get_product(args, state):
    p = PRODUCT_BY_ID.get(args['product_id'])
    if p is None:
        raise _not_found(f'product {args["product_id"]}')
    return p


@tool('list_products', 'List all products, optionally of one category.',
      {'category': ('string', 'Optional: hardware, software, accessories or services')},
      'a list of {"id": str, "name": str, "category": str, "price": float, "stock": int}', required=[])
def _list_products(args, state):
    return [p for p in PRODUCTS if args.get('category') in (None, p['category'])]


FX = {('USD', 'EUR'): 0.9213, ('USD', 'GBP'): 0.7841, ('USD', 'CAD'): 1.3612, ('USD', 'JPY'): 148.27,
      ('EUR', 'USD'): 1.0854, ('GBP', 'USD'): 1.2753, ('CAD', 'USD'): 0.7347, ('JPY', 'USD'): 0.006744,
      ('EUR', 'GBP'): 0.8511, ('GBP', 'EUR'): 1.1749, ('USD', 'CHF'): 0.8822, ('USD', 'AUD'): 1.5234}


@tool('get_exchange_rate', 'Current exchange rate between two ISO currency codes.',
      {'base': ('string', 'Currency to convert from, e.g. "USD"'),
       'quote': ('string', 'Currency to convert to, e.g. "EUR"')},
      'an object {"base": str, "quote": str, "rate": float} where 1 base = rate quote')
def _get_exchange_rate(args, state):
    _maybe_fail('get_exchange_rate', args['quote'], state)
    rate = FX.get((args['base'], args['quote']))
    if rate is None:
        raise ToolError(f'unsupported currency pair {args["base"]}/{args["quote"]}', 'ValueError')
    return {'base': args['base'], 'quote': args['quote'], 'rate': rate}


@tool('send_email', 'Send an email.',
      {'to': ('string', 'Recipient address'), 'subject': ('string', 'Subject line'),
       'body': ('string', 'Plain-text body')},
      'an object {"status": "queued", "message_id": str}')
def _send_email(args, state):
    n = state['counts']['send_email']
    return {'status': 'queued', 'message_id': f'msg-{n:04d}'}


# ==========================================================================
# Failure injection shared by tools: config["fail"][tool][key] = [kinds...]
# raises one error per entry on that key's first calls.


def _maybe_fail(name, key, state):
    plan = state['config'].get('fail', {}).get(name, {})
    errors = plan.get(str(key), plan.get('*'))
    if not errors:
        return
    seen = state.setdefault('fail_seen', {})
    k = (name, str(key))
    n = seen.get(k, 0)
    seen[k] = n + 1
    if n < len(errors):
        kind, msg = errors[n]
        raise ToolError(msg, kind)


# ==========================================================================
# Logs

SERVICES = ['auth', 'billing', 'search', 'gateway', 'notifications']
LOG_DATES = ['2026-03-13', '2026-03-14', '2026-03-15']


def _logs():
    rnd = random.Random(777)
    out = {}
    for s in SERVICES:
        for d in LOG_DATES:
            lines = []
            t = dt.datetime.fromisoformat(d + 'T00:00:00')
            for _ in range(120):
                t += dt.timedelta(seconds=rnd.randrange(30, 1400), milliseconds=rnd.randrange(1000))
                if t.date().isoformat() != d:
                    break
                ts = t.strftime('%Y-%m-%dT%H:%M:%S.') + f'{t.microsecond // 1000:03d}Z'
                pid = 4100 + SERVICES.index(s)
                user = f'u{rnd.randrange(1, 60):02d}'
                r = rnd.random()
                lat = int(rnd.lognormvariate(4.5, 0.6))
                if r < 0.62:
                    path = rnd.choice(['/api/login', '/api/items', '/api/search', '/api/cart', '/api/pay'])
                    status = rnd.choices([200, 201, 204, 400, 404, 500, 503], [60, 8, 6, 8, 8, 6, 4])[0]
                    lvl = 'INFO' if status < 400 else ('WARN' if status < 500 else 'ERROR')
                    msg = f'request completed method=GET path={path} status={status} latency_ms={lat} user={user}'
                elif r < 0.72:
                    lvl = 'WARN'
                    msg = f'slow query took {lat * 10}ms table={rnd.choice(["users", "orders", "sessions"])}'
                elif r < 0.80:
                    lvl = 'ERROR'
                    up = rnd.choice([x for x in SERVICES if x != s])
                    msg = f'upstream timeout calling {up} after {rnd.choice([1000, 2000, 3000])}ms'
                elif r < 0.86:
                    lvl = 'ERROR'
                    ex = rnd.choice(["KeyError: 'sku'", 'ValueError: invalid literal for int()',
                                     'ConnectionResetError: [Errno 104] Connection reset by peer'])
                    msg = f'unhandled exception in worker: {ex}'
                elif r < 0.93:
                    lvl = 'WARN' if s == 'auth' else 'INFO'
                    ip = f'10.0.{rnd.randrange(1, 5)}.{rnd.randrange(2, 250)}'
                    msg = (f'login failed user={user} ip={ip} reason={rnd.choice(["bad_password", "locked", "mfa_timeout"])}'
                           if s == 'auth' else f'cache refresh finished entries={rnd.randrange(100, 9000)}')
                else:
                    lvl = 'DEBUG'
                    msg = f'heartbeat ok seq={rnd.randrange(10000)}'
                lines.append(f'{ts} {lvl:<5} {s}[{pid}]: {msg}')
            out[(s, d)] = lines
    return out


LOGS = _logs()


def parse_log_line(line):
    ts, lvl, rest = line[:24], line[25:30].strip(), line[31:]
    svc, msg = rest.split(': ', 1)
    return ts, lvl, svc.split('[')[0], msg


@tool('list_services', 'List the names of all monitored services.', {},
      'a list of service names (strings)', required=[])
def _list_services(args, state):
    return list(SERVICES)


@tool('fetch_logs', 'Fetch one day of a service\'s log as text. Each line looks like '
      '"2026-03-14T08:01:02.123Z ERROR auth[4100]: message key=value ...". '
      'Levels are DEBUG, INFO, WARN and ERROR (padded to 5 characters). Logs exist for '
      '2026-03-13 to 2026-03-15.',
      {'service': ('string', 'Service name'), 'date': ('string', 'Day, YYYY-MM-DD')},
      'the log text, one entry per line', text=True)
def _fetch_logs(args, state):
    _maybe_fail('fetch_logs', args['service'], state)
    lines = LOGS.get((args['service'], args['date']))
    if lines is None:
        raise _not_found(f'no logs for {args["service"]} on {args["date"]}')
    return '\n'.join(lines) + '\n'


@tool('query_logs', 'Query structured log entries of one service across all days, oldest first, 50 per page.',
      {'service': ('string', 'Service name'),
       'level': ('string', 'Optional minimum level: DEBUG, INFO, WARN or ERROR'),
       'page_token': ('string', 'Token from the previous page\'s next_page_token')},
      'an object {"entries": [{"ts": "2026-03-14T08:01:02.123Z", "level": str, "message": str}], '
      '"next_page_token": str or null}', required=['service'])
def _query_logs(args, state):
    order = ['DEBUG', 'INFO', 'WARN', 'ERROR']
    lvl = args.get('level', 'DEBUG')
    if lvl not in order:
        raise ToolError(f'bad level {lvl!r}', 'ValueError')
    rows = []
    for d in LOG_DATES:
        for line in LOGS.get((args['service'], d), []):
            ts, lv, _, msg = parse_log_line(line)
            if order.index(lv) >= order.index(lvl):
                rows.append({'ts': ts, 'level': lv, 'message': msg})
    if not rows and args['service'] not in SERVICES:
        raise _not_found(f'service {args["service"]}')
    start = int(args.get('page_token') or 0)
    nxt = str(start + 50) if start + 50 < len(rows) else None
    return {'entries': rows[start:start + 50], 'next_page_token': nxt}


def _access_log():
    rnd = random.Random(31337)
    lines = []
    t = dt.datetime(2026, 3, 14, 0, 0, 0)
    agents = ['Mozilla/5.0 (X11; Linux x86_64)', 'curl/8.5.0', 'Googlebot/2.1 (+http://www.google.com/bot.html)',
              'python-requests/2.32.3', 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X)']
    paths = ['/', '/index.html', '/products?id=12', '/products?id=7&ref=mail', '/login', '/api/v1/items',
             '/static/app.js', '/static/style.css', '/search?q=usb+hub', '/search?q=laptop%20pro', '/admin',
             '/wp-login.php', '/checkout']
    for _ in range(400):
        t += dt.timedelta(seconds=rnd.randrange(1, 400))
        ip = rnd.choice(['203.0.113.7', '198.51.100.23', '192.0.2.44', '203.0.113.99', '198.51.100.5',
                         f'10.1.{rnd.randrange(3)}.{rnd.randrange(2, 30)}'])
        method = rnd.choices(['GET', 'POST', 'HEAD'], [80, 15, 5])[0]
        path = rnd.choice(paths)
        status = rnd.choices([200, 301, 304, 401, 403, 404, 500], [65, 5, 8, 5, 4, 9, 4])[0]
        size = 0 if status in (301, 304) else rnd.randrange(200, 50000)
        ref = rnd.choice(['-', 'https://example.com/', 'https://www.google.com/'])
        lines.append(f'{ip} - - [{t.strftime("%d/%b/%Y:%H:%M:%S")} +0000] "{method} {path} HTTP/1.1" '
                     f'{status} {size} "{ref}" "{rnd.choice(agents)}"')
    return lines


ACCESS_LOG = _access_log()


@tool('get_access_log', 'Fetch the web server access log for 2026-03-14 in Combined Log Format: '
      '`IP - - [14/Mar/2026:08:01:02 +0000] "GET /path HTTP/1.1" STATUS BYTES "REFERER" "USER-AGENT"`.',
      {'date': ('string', 'Day, YYYY-MM-DD (only 2026-03-14 exists)')},
      'the log text, one request per line', text=True)
def _get_access_log(args, state):
    if args['date'] != '2026-03-14':
        raise _not_found(f'no access log for {args["date"]}')
    return '\n'.join(ACCESS_LOG) + '\n'


@tool('create_incident', 'Open an incident.',
      {'title': ('string', 'Short title'), 'severity': ('string', 'sev1, sev2 or sev3'),
       'services': ('array', 'Affected service names', {'items': {'type': 'string'}}),
       'summary': ('string', 'Optional longer description')},
      'an object {"incident_id": str, "status": "open"}', required=['title', 'severity', 'services'])
def _create_incident(args, state):
    return {'incident_id': f'INC-{500 + state["counts"]["create_incident"]}', 'status': 'open'}


@tool('post_message', 'Post a message to a chat channel.',
      {'channel': ('string', 'Channel name like "#ops"'), 'text': ('string', 'Message text (Markdown allowed)')},
      'an object {"ok": true, "ts": str}')
def _post_message(args, state):
    return {'ok': True, 'ts': f'1773576000.{state["counts"]["post_message"]:06d}'}


# ==========================================================================
# Files

def _files():
    rnd = random.Random(4242)
    files = {}
    # Quarterly sales, one row per sale.
    reps = ['Alice Wong', 'Bob Diaz', 'Chen Li', 'Dana Fox', 'Eve Stone']
    rows = ['date,rep,region,product,units,unit_price']
    for _ in range(120):
        d = dt.date(2026, 1, 1) + dt.timedelta(days=rnd.randrange(90))
        p = rnd.choice(PRODUCTS[:12])
        rows.append(f'{d.isoformat()},{rnd.choice(reps)},{rnd.choice(REGIONS)},{p["name"]},'
                    f'{rnd.randrange(1, 12)},{p["price"]:.2f}')
    files['/data/sales_2026q1.csv'] = '\n'.join(rows) + '\n'
    # Employees, with quoted fields.
    depts = ['Engineering', 'Sales', 'Support', 'Finance', 'Marketing']
    rows = ['id,name,department,title,salary,start_date,manager_id,location']
    for i in range(1, 51):
        first, last = FIRST[(i * 3) % 20], LAST[(i * 7) % 10]
        dept = depts[rnd.randrange(5)]
        title = rnd.choice(['Engineer', 'Senior Engineer', 'Manager', 'Analyst', 'Specialist', 'Lead, Platform'])
        title = f'"{title}"' if ',' in title else title
        loc = rnd.choice(['"Austin, TX"', '"Berlin, DE"', '"Toronto, ON"', 'Remote'])
        mgr = '' if i <= 5 else str(rnd.randrange(1, 6))
        rows.append(f'{i},{first} {last},{dept},{title},{rnd.randrange(55, 190) * 1000},'
                    f'{(dt.date(2016, 1, 1) + dt.timedelta(days=rnd.randrange(3600))).isoformat()},{mgr},{loc}')
    files['/data/employees.csv'] = '\n'.join(rows) + '\n'
    # Inventory as nested JSON.
    whs = []
    for w in ['AUS-1', 'BER-2', 'TOR-3']:
        whs.append({'warehouse': w, 'updated': '2026-03-14T18:00:00Z',
                    'bins': [{'sku': p['id'], 'name': p['name'], 'qty': rnd.randrange(0, 60),
                              'reorder_level': rnd.choice([5, 10, 15])}
                             for p in rnd.sample(PRODUCTS, 10)]})
    files['/data/inventory.json'] = json.dumps({'warehouses': whs}, indent=2)
    # Users as JSON lines with optional fields.
    lines = []
    for i in range(1, 31):
        u = {'id': i, 'username': f'user{i:02d}', 'email': f'user{i:02d}@corp.example',
             'roles': rnd.sample(['admin', 'dev', 'ops', 'viewer', 'billing'], rnd.randrange(1, 3)),
             'active': rnd.random() > 0.2, 'last_login': (dt.datetime(2026, 1, 1) + dt.timedelta(
                 minutes=rnd.randrange(105 * 1440))).isoformat() + 'Z'}
        if rnd.random() < 0.3:
            del u['last_login']
        if rnd.random() < 0.3:
            u['team'] = rnd.choice(['core', 'web', 'data'])
        lines.append(json.dumps(u))
    files['/data/users.jsonl'] = '\n'.join(lines) + '\n'
    # European CSV: semicolons and decimal commas.
    rows = ['Datum;Konto;Betrag;Waehrung;Verwendungszweck']
    for _ in range(40):
        d = dt.date(2026, 2, 1) + dt.timedelta(days=rnd.randrange(28))
        amt = rnd.randrange(-250000, 150000) / 100
        betrag = f'{amt:.2f}'.replace('.', ',')
        konto = rnd.choice(['DE11 1111', 'DE22 2222', 'DE33 3333'])
        zweck = rnd.choice(['Miete', 'Gehalt', 'Strom', 'Einkauf', 'Versicherung'])
        rows.append(f'{d.strftime("%d.%m.%Y")};{konto};{betrag};EUR;{zweck}')
    files['/data/bank_feb.csv'] = '\n'.join(rows) + '\n'
    # Survey with free text, quotes and embedded newlines.
    rows = ['respondent,score,comment']
    comments = ['Great product', 'Too expensive, but works', 'Support was "very" helpful',
                'Setup took long\nbut fine afterwards', '', 'Would recommend', 'Crashes often, please fix']
    for i in range(1, 26):
        c = rnd.choice(comments)
        q = '"' + c.replace('"', '""') + '"' if any(ch in c for ch in ',"\n') else c
        rows.append(f'R{i:03d},{rnd.randrange(1, 11)},{q}')
    files['/data/survey.csv'] = '\n'.join(rows) + '\n'
    # Key=value config and an INI-like file.
    files['/etc/app.conf'] = ('# app config\nname = billing-api\nport = 8443\ndebug = false\n'
                              'timeout_seconds = 30\nallowed_hosts = api.example.com, admin.example.com\n'
                              '\n[database]\nhost = db.internal\nport = 5432\npool_size = 12\n'
                              '\n[cache]\nhost = redis.internal\nttl = 300\n')
    files['/etc/settings.toml'] = ('title = "billing"\nversion = "2.4.1"\n\n[server]\nhost = "0.0.0.0"\n'
                                   'port = 8443\nworkers = 4\n\n[limits]\nmax_upload_mb = 25\n'
                                   'rate_per_minute = 600\n\n[[features]]\nname = "invoices"\nenabled = true\n\n'
                                   '[[features]]\nname = "refunds"\nenabled = false\n')
    # Base64 payload and checksums.
    payload = json.dumps({'batch': 'B-2026-03-14', 'records': [{'id': i, 'amount': round(rnd.random() * 100, 2)}
                                                               for i in range(1, 8)]})
    files['/inbox/payload.b64'] = base64.b64encode(payload.encode()).decode() + '\n'
    for name, body in (('report.txt', 'Quarterly report\nAll systems nominal.\n'),
                       ('data.bin.txt', 'x' * 100 + '\n'),
                       ('notes.md', '# Notes\n\n- one\n- two\n')):
        files[f'/release/{name}'] = body
    sums = [f'{hashlib.sha256(files[f"/release/{n}"].encode()).hexdigest()}  {n}' for n in
            ('report.txt', 'data.bin.txt')]
    sums.append(f'{"0" * 64}  notes.md')
    files['/release/SHA256SUMS'] = '\n'.join(sums) + '\n'
    # A markdown changelog and a text document.
    files['/docs/CHANGELOG.md'] = (
        '# Changelog\n\n## [2.4.1] - 2026-03-10\n### Fixed\n- Rounding of invoice totals (#412)\n'
        '- Crash on empty cart (#415)\n\n## [2.4.0] - 2026-02-20\n### Added\n- Refund API (#398)\n'
        '- CSV export for invoices (#401)\n### Changed\n- Faster search indexing (#390)\n\n'
        '## [2.3.2] - 2026-01-28\n### Fixed\n- Timezone bug in reports (#377)\n\n'
        '## [2.3.0] - 2026-01-05\n### Added\n- Multi-currency support (#350)\n- Webhooks (#356)\n'
        '### Removed\n- Legacy XML endpoint (#360)\n')
    files['/docs/contract.txt'] = (
        'SERVICE AGREEMENT\n\nThis agreement is made on March 2, 2026 between Acme Corp '
        '(contact: legal@acme.example, +1 (555) 010-2233) and Globex Ltd (contact: '
        'contracts@globex.example, +44 20 7946 0958).\n\nThe term starts on 2026-04-01 and ends on '
        '2027-03-31. Fees: USD 12,500.00 per quarter, payable within 30 days. A late fee of 1.5% '
        'applies per month. Support hours: Mon-Fri 08:00-18:00 CET.\n\nInvoices INV-2026-0042, '
        'INV-2026-0057 and INV-2026-0061 are covered by this agreement. Questions: '
        'billing@acme.example.\n')
    return files


FILES = _files()


@tool('list_files', 'List files under a directory prefix.',
      {'prefix': ('string', 'Directory prefix such as "/data/"')},
      'a list of {"path": str, "size": int} objects (size in bytes)')
def _list_files(args, state):
    return [{'path': p, 'size': len(c.encode())} for p, c in sorted(FILES.items())
            if p.startswith(args['prefix'])]


@tool('read_file', 'Read a text file.', {'path': ('string', 'Absolute path')},
      'the file content as a string', text=True)
def _read_file(args, state):
    content = state.setdefault('written', {}).get(args['path'], FILES.get(args['path']))
    if content is None:
        raise ToolError(f'no such file: {args["path"]}', 'FileNotFoundError')
    return content


@tool('write_file', 'Write (create or replace) a text file.',
      {'path': ('string', 'Absolute path'), 'content': ('string', 'Full new content')},
      'an object {"path": str, "bytes_written": int}')
def _write_file(args, state):
    if not isinstance(args['content'], str):
        raise ToolError('content must be a string', 'TypeError')
    state.setdefault('written', {})[args['path']] = args['content']
    return {'path': args['path'], 'bytes_written': len(args['content'].encode())}


# ==========================================================================
# Calendar and time

USERS_TZ = {
    'alice': ('Alice Wong', 'America/New_York', '09:00', '17:00'),
    'bjorn': ('Bjorn Berg', 'Europe/Stockholm', '08:00', '16:00'),
    'chitra': ('Chitra Rao', 'Asia/Kolkata', '10:00', '18:00'),
    'diego': ('Diego Ruiz', 'America/Los_Angeles', '09:00', '17:00'),
    'emi': ('Emi Sato', 'Asia/Tokyo', '09:30', '18:30'),
}


@tool('get_user_profile', 'Fetch a user\'s profile.', {'user': ('string', 'Username, e.g. "alice"')},
      'an object {"user": str, "name": str, "timezone": IANA zone name like "Europe/Berlin", '
      '"working_hours": {"start": "HH:MM", "end": "HH:MM"} in the user\'s local time}')
def _get_user_profile(args, state):
    u = USERS_TZ.get(args['user'])
    if u is None:
        raise _not_found(f'user {args["user"]}')
    return {'user': args['user'], 'name': u[0], 'timezone': u[1],
            'working_hours': {'start': u[2], 'end': u[3]}}


def _events():
    rnd = random.Random(99)
    evs = []
    titles = ['Standup', 'Design review', '1:1', 'Sprint planning', 'Customer call', 'Lunch & learn',
              'Incident retro', 'Hiring sync', 'Budget review', 'All hands']
    offsets = ['-05:00', '-04:00', '+00:00', '+01:00', '+05:30', '+09:00']
    for i in range(1, 61):
        day = dt.date(2026, 3, 9) + dt.timedelta(days=rnd.randrange(14))
        off = rnd.choice(offsets)
        tz = dt.timezone(dt.timedelta(hours=int(off[:3]), minutes=int(off[0] + off[4:])))
        start = dt.datetime.combine(day, dt.time(rnd.randrange(7, 19), rnd.choice([0, 15, 30, 45])), tz)
        dur = rnd.choice([15, 30, 30, 45, 60, 60, 90])
        evs.append({'id': f'evt-{i:03d}', 'title': rnd.choice(titles),
                    'start': start.isoformat(), 'end': (start + dt.timedelta(minutes=dur)).isoformat(),
                    'attendees': sorted(rnd.sample(list(USERS_TZ), rnd.randrange(1, 4))),
                    'location': rnd.choice(['Zoom', 'Room 4B', 'Room 2A', 'Phone'])})
    return evs


EVENTS = _events()


@tool('list_events', 'List calendar events that start in [start, end). Times are ISO 8601 with UTC offsets.',
      {'start': ('string', 'ISO 8601 date or datetime (UTC if no offset)'),
       'end': ('string', 'ISO 8601 date or datetime (UTC if no offset)'),
       'attendee': ('string', 'Optional username filter')},
      'a list of {"id": str, "title": str, "start": str, "end": str, "attendees": [usernames], '
      '"location": str}, ordered by start', required=['start', 'end'])
def _list_events(args, state):
    def parse(s):
        d = dt.datetime.fromisoformat(s.replace('Z', '+00:00'))
        return d if d.tzinfo else d.replace(tzinfo=dt.UTC)
    try:
        lo, hi = parse(args['start']), parse(args['end'])
    except ValueError as e:
        raise ToolError(f'bad datetime: {e}', 'ValueError') from None
    out = [e for e in EVENTS if lo <= dt.datetime.fromisoformat(e['start']) < hi
           and args.get('attendee') in (None, *e['attendees'])]
    return sorted(out, key=lambda e: (dt.datetime.fromisoformat(e['start']), e['id']))


@tool('create_event', 'Create a calendar event.',
      {'title': ('string', 'Title'), 'start': ('string', 'ISO 8601 datetime with offset'),
       'end': ('string', 'ISO 8601 datetime with offset'),
       'attendees': ('array', 'Usernames', {'items': {'type': 'string'}})},
      'an object {"id": str, "status": "created"}')
def _create_event(args, state):
    return {'id': f'evt-new-{state["counts"]["create_event"]}', 'status': 'created'}


HOLIDAYS = {('US', 2026): ['2026-01-01', '2026-01-19', '2026-02-16', '2026-05-25', '2026-06-19', '2026-07-03',
                           '2026-09-07', '2026-10-12', '2026-11-11', '2026-11-26', '2026-12-25'],
            ('DE', 2026): ['2026-01-01', '2026-04-03', '2026-04-06', '2026-05-01', '2026-05-14', '2026-05-25',
                           '2026-10-03', '2026-12-25', '2026-12-26'],
            ('GB', 2026): ['2026-01-01', '2026-04-03', '2026-04-06', '2026-05-04', '2026-05-25', '2026-08-31',
                           '2026-12-25', '2026-12-28'],
            ('US', 2027): ['2027-01-01', '2027-01-18', '2027-02-15', '2027-05-31', '2027-06-18', '2027-07-05',
                           '2027-09-06', '2027-10-11', '2027-11-11', '2027-11-25', '2027-12-24'],
            ('DE', 2027): ['2027-01-01', '2027-03-26', '2027-03-29', '2027-05-01', '2027-05-06', '2027-05-17',
                           '2027-10-03', '2027-12-25', '2027-12-26'],
            ('GB', 2027): ['2027-01-01', '2027-03-26', '2027-03-29', '2027-05-03', '2027-05-31', '2027-08-30',
                           '2027-12-27', '2027-12-28']}


@tool('list_holidays', 'Public holidays of a country in a year.',
      {'country': ('string', 'ISO country code: US, DE or GB'), 'year': ('integer', 'Year')},
      'a list of dates as "YYYY-MM-DD" strings')
def _list_holidays(args, state):
    h = HOLIDAYS.get((args['country'], args['year']))
    if h is None:
        raise _not_found(f'holidays for {args["country"]} {args["year"]}')
    return h


def _subscriptions():
    rnd = random.Random(5150)
    subs = []
    for i in range(1, 26):
        start = dt.date(2025, 1, 1) + dt.timedelta(days=rnd.randrange(430))
        plan = rnd.choice(['monthly', 'monthly', 'annual', 'quarterly'])
        subs.append({'id': f'sub-{i:03d}', 'customer': CUSTOMERS[i]['name'], 'plan': plan,
                     'started': start.isoformat(), 'price': {'monthly': 19.0, 'quarterly': 51.0, 'annual': 190.0}[plan],
                     'trial_days': rnd.choice([0, 0, 7, 14, 30]),
                     'cancelled_at': None if rnd.random() > 0.2 else (start + dt.timedelta(days=rnd.randrange(20, 300))).isoformat()})
    return subs


SUBSCRIPTIONS = _subscriptions()


@tool('list_subscriptions', 'List all subscriptions.', {},
      'a list of {"id": str, "customer": str, "plan": "monthly"|"quarterly"|"annual", "started": "YYYY-MM-DD", '
      '"price": float per billing period, "trial_days": int, "cancelled_at": "YYYY-MM-DD" or null}', required=[])
def _list_subscriptions(args, state):
    return SUBSCRIPTIONS


def _deploys():
    rnd = random.Random(808)
    out = []
    t = 1772323200  # 2026-03-01T00:00:00Z
    for i in range(40):
        t += rnd.randrange(3600, 86400)
        out.append({'id': f'dep-{i + 1:03d}', 'service': rnd.choice(SERVICES), 'started_at': t,
                    'duration_s': rnd.randrange(40, 1900), 'status': rnd.choices(['success', 'failed', 'rolled_back'], [80, 12, 8])[0],
                    'author': rnd.choice(list(USERS_TZ))})
    return out


DEPLOYS = _deploys()


@tool('list_deployments', 'List deployments since 2026-03-01, oldest first. Times are Unix epoch seconds (UTC).',
      {'service': ('string', 'Optional service filter')},
      'a list of {"id": str, "service": str, "started_at": int epoch seconds, "duration_s": int, '
      '"status": "success"|"failed"|"rolled_back", "author": str}', required=[])
def _list_deployments(args, state):
    return [d for d in DEPLOYS if args.get('service') in (None, d['service'])]


# ==========================================================================
# Web

PAGES = {}


def _pages():
    PAGES['https://shop.example.com/'] = (200, 'text/html', (
        '<html><head><title>Example Shop</title></head><body>\n<h1>Welcome</h1>\n'
        '<a href="/products?id=12">USB-C Hub</a> <a href="/products?id=7&amp;ref=home">Laptop Air</a>\n'
        '<a href="https://blog.example.com/post/1">Blog</a> <a href="/about">About us</a>\n'
        '<a href="mailto:sales@shop.example.com">Sales</a> <a href="/products?id=12">Hub again</a>\n'
        '<p>Contact support@shop.example.com or call +1-555-0100.</p>\n'
        '<a href="https://partner.example.org/ref?src=shop">Partner</a>\n</body></html>\n'))
    PAGES['https://shop.example.com/about'] = (200, 'text/html', (
        '<html><head><title>About &amp; Team</title></head><body><h1>About</h1>'
        '<p>Founded in 2019. Email jobs@shop.example.com, press@shop.example.com.</p>'
        '<p>Office: 12 Main St, Springfield. Phone +1-555-0199.</p>'
        '<a href="/">Home</a><a href="/team">Team</a></body></html>\n'))
    PAGES['https://shop.example.com/team'] = (404, 'text/html', '<html><body>Not Found</body></html>\n')
    PAGES['https://shop.example.com/robots.txt'] = (200, 'text/plain', (
        'User-agent: *\nDisallow: /admin\nDisallow: /checkout\nAllow: /products\n\n'
        'User-agent: BadBot\nDisallow: /\n\nSitemap: https://shop.example.com/sitemap.xml\n'))
    PAGES['https://shop.example.com/sitemap.xml'] = (200, 'application/xml', (
        '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
        + ''.join(f'  <url><loc>https://shop.example.com/products?id={i}</loc><lastmod>2026-03-{i:02d}</lastmod></url>\n'
                  for i in range(1, 13)) + '</urlset>\n'))
    for i in range(1, 13):
        p = PRODUCTS[i - 1]
        PAGES[f'https://shop.example.com/products?id={i}'] = (200, 'text/html', (
            f'<html><head><title>{p["name"]} | Example Shop</title>'
            f'<meta name="description" content="Buy the {p["name"]} today"></head><body>'
            f'<h1 class="title">{p["name"]}</h1><span class="price">${p["price"]:,.2f}</span>'
            f'<span class="sku">SKU: {p["id"]}</span><div class="stock">{p["stock"]} in stock</div>'
            f'<ul class="reviews">' + ''.join(f'<li data-rating="{(i * k) % 5 + 1}">review {k}</li>'
                                              for k in range(1, 1 + i % 4 + 1)) + '</ul></body></html>\n'))
    PAGES['https://api.example.com/v1/status'] = (200, 'application/json', json.dumps({
        'components': [{'name': n, 'status': s, 'uptime_pct': u} for n, s, u in (
            ('api', 'operational', 99.98), ('db', 'degraded', 99.41), ('cdn', 'operational', 99.995),
            ('auth', 'outage', 97.2), ('search', 'operational', 99.9))],
        'updated': '2026-03-15T11:55:00Z'}))
    PAGES['https://news.example.com/feed.xml'] = (200, 'application/rss+xml', (
        '<?xml version="1.0"?><rss><channel><title>Example News</title>'
        + ''.join(f'<item><title>{t}</title><link>https://news.example.com/{s}</link>'
                  f'<pubDate>{d}</pubDate></item>' for t, s, d in (
                      ('Rates hold steady', 'rates', 'Fri, 13 Mar 2026 09:00:00 GMT'),
                      ('New chip &amp; board launch', 'chips', 'Sat, 14 Mar 2026 15:30:00 GMT'),
                      ('Storm warning issued', 'storm', 'Sun, 15 Mar 2026 06:45:00 GMT'),
                      ('Markets rally', 'rally', 'Thu, 12 Mar 2026 17:10:00 GMT')))
        + '</channel></rss>\n'))
    PAGES['https://api.example.com/v1/token'] = (200, 'application/json', json.dumps({
        'access_token': base64.urlsafe_b64encode(b'{"alg":"HS256","typ":"JWT"}').rstrip(b'=').decode() + '.'
        + base64.urlsafe_b64encode(json.dumps({'sub': 'svc-reporter', 'scope': 'read:orders read:users',
                                               'exp': 1773590400, 'iat': 1773576000}).encode()).rstrip(b'=').decode()
        + '.c2lnbmF0dXJl', 'token_type': 'Bearer'}))


_pages()


@tool('http_get', 'HTTP GET a URL from the allow-listed hosts (shop.example.com, api.example.com, '
      'news.example.com). Query strings are part of the URL.',
      {'url': ('string', 'Absolute URL')},
      'an object {"status": int, "content_type": str, "body": str}')
def _http_get(args, state):
    url = args['url']
    _maybe_fail('http_get', url, state)
    parts = urllib.parse.urlsplit(url)
    if parts.scheme != 'https' or parts.netloc not in ('shop.example.com', 'api.example.com', 'news.example.com'):
        raise ToolError(f'host not allowed: {parts.netloc or url}', 'PermissionError')
    key = urllib.parse.urlunsplit((parts.scheme, parts.netloc, parts.path or '/', parts.query, ''))
    if key in PAGES:
        st, ct, body = PAGES[key]
        return {'status': st, 'content_type': ct, 'body': body}
    if parts.path == '/api/search':
        q = urllib.parse.parse_qs(parts.query)
        term = q.get('q', [''])[0].lower()
        hits = [{'id': p['id'], 'name': p['name'], 'price': p['price']} for p in PRODUCTS if term and term in p['name'].lower()]
        return {'status': 200, 'content_type': 'application/json', 'body': json.dumps({'query': term, 'results': hits})}
    return {'status': 404, 'content_type': 'text/html', 'body': '<html><body>Not Found</body></html>\n'}


# ==========================================================================
# Metrics

HOSTS = [f'web-{i:02d}' for i in range(1, 9)] + [f'db-{i:02d}' for i in range(1, 4)]


def metric_points(host, metric):
    seed = int(hashlib.sha256(f'{host}/{metric}'.encode()).hexdigest()[:8], 16)
    rnd = random.Random(seed)
    base = {'cpu': 35 + seed % 30, 'mem': 50 + seed % 35, 'latency_ms': 80 + seed % 120,
            'errors': 1 + seed % 4}[metric]
    pts = []
    t0 = 1773532800  # 2026-03-15T00:00:00Z
    for k in range(48):  # every 15 minutes, 12 hours
        v = base + 12 * math.sin(k / 7 + seed % 5) + rnd.gauss(0, 4)
        if metric == 'errors':
            v = max(0, round(base + rnd.gauss(0, 1.5) + (9 if 30 <= k <= 33 and seed % 3 == 0 else 0)))
        elif metric == 'cpu':
            v = min(100.0, max(0.0, round(v + (35 if 30 <= k <= 33 and seed % 3 == 0 else 0), 1)))
        else:
            v = round(max(0.0, v), 1)
        if rnd.random() < 0.04:
            v = None
        pts.append([t0 + 900 * k, v])
    return pts


@tool('list_hosts', 'List hosts of the production cluster.', {},
      'a list of {"host": str, "role": "web"|"db", "zone": str}', required=[])
def _list_hosts(args, state):
    return [{'host': h, 'role': h.split('-')[0], 'zone': ['us-east-1a', 'us-east-1b', 'us-east-1c'][i % 3]}
            for i, h in enumerate(HOSTS)]


@tool('get_metrics', 'Time series of one metric for one host on 2026-03-15 00:00-12:00 UTC, one point per '
      '15 minutes. Missing samples are null.',
      {'host': ('string', 'Host name'), 'metric': ('string', 'cpu (percent), mem (percent), latency_ms or errors (count)')},
      'an object {"host": str, "metric": str, "points": [[epoch_seconds int, value float or null], ...]}')
def _get_metrics(args, state):
    _maybe_fail('get_metrics', args['host'], state)
    if args['host'] not in HOSTS:
        raise _not_found(f'host {args["host"]}')
    if args['metric'] not in ('cpu', 'mem', 'latency_ms', 'errors'):
        raise ToolError(f'unknown metric {args["metric"]!r}', 'ValueError')
    return {'host': args['host'], 'metric': args['metric'], 'points': metric_points(args['host'], args['metric'])}


# ==========================================================================
# Tickets

TEAM = {'alice': 'Alice Wong', 'bjorn': 'Bjorn Berg', 'chitra': 'Chitra Rao', 'diego': 'Diego Ruiz', 'emi': 'Emi Sato'}


def _issues():
    rnd = random.Random(2468)
    out = []
    titles = ['Login page times out', 'Invoice PDF missing logo', 'Search returns duplicates', 'Payment webhook retries forever',
              'Dark mode contrast too low', 'Export to CSV drops unicode', 'Memory leak in worker', 'Password reset email delayed',
              'API rate limit too strict', 'Typo on pricing page', 'Crash when cart is empty', 'Slow dashboard load',
              'Wrong tax for Canada', 'SSO redirect loop', 'Flaky integration test', 'Upgrade TLS library']
    for i in range(60):
        created = dt.datetime(2026, 1, 5, 9) + dt.timedelta(hours=rnd.randrange(1600))
        updated = created + dt.timedelta(hours=rnd.randrange(1, 900))
        if updated > dt.datetime(2026, 3, 15, 11):
            updated = dt.datetime(2026, 3, 15, 11) - dt.timedelta(minutes=rnd.randrange(1, 600))
        status = rnd.choices(['open', 'in_progress', 'closed'], [40, 25, 35])[0]
        out.append({
            'key': f'OPS-{101 + i}', 'title': rnd.choice(titles), 'status': status,
            'priority': rnd.choices(['P1', 'P2', 'P3', 'P4'], [10, 25, 40, 25])[0],
            'assignee': rnd.choice([None, *TEAM]), 'labels': sorted(rnd.sample(['bug', 'ui', 'backend', 'billing', 'security', 'infra', 'customer'], rnd.randrange(0, 3))),
            'created': created.isoformat() + 'Z', 'updated': updated.isoformat() + 'Z',
            'estimate_hours': rnd.choice([1, 2, 3, 5, 8, 13, None]),
            'comments': [{'author': rnd.choice(list(TEAM)), 'body': rnd.choice(['Looking into it', 'Can repro on staging',
                                                                                 'Blocked on vendor', 'Fixed in #4521?', 'Customer escalated'])}
                         for _ in range(rnd.randrange(0, 4))]})
    return out


ISSUES = _issues()
ISSUE_BY_KEY = {i['key']: i for i in ISSUES}


def _match(issue, query):
    for term in query.split():
        if ':' in term:
            k, v = term.split(':', 1)
            if k == 'label':
                if v not in issue['labels']:
                    return False
            elif k in ('status', 'priority', 'assignee'):
                if str(issue[k]) != v:
                    return False
            else:
                raise ToolError(f'unknown field {k!r} in query', 'ValueError')
        elif term.lower() not in issue['title'].lower():
            return False
    return True


@tool('search_issues', 'Search issues with a query such as "status:open label:bug login". Terms with a colon '
      'filter on status (open, in_progress, closed), priority (P1-P4), assignee (username or None) or '
      'label; other words must appear in the title. 20 results per page, ordered by key.',
      {'query': ('string', 'The query; empty matches everything'), 'page': ('integer', 'Page number from 1 (default 1)')},
      'an object {"issues": [{"key": str, "title": str, "status": str, "priority": str, "assignee": str or null, '
      '"labels": [str], "created": ISO str, "updated": ISO str, "estimate_hours": int or null}], '
      '"page": int, "has_more": bool}', required=['query'])
def _search_issues(args, state):
    _maybe_fail('search_issues', args.get('page', 1), state)
    hits = [{k: v for k, v in i.items() if k != 'comments'} for i in ISSUES if _match(i, args['query'])]
    page = args.get('page', 1)
    return {'issues': hits[(page - 1) * 20:page * 20], 'page': page, 'has_more': page * 20 < len(hits)}


@tool('get_issue', 'Fetch one issue with its comments.', {'key': ('string', 'Issue key like "OPS-123"')},
      'an object with the search fields plus "comments": [{"author": str, "body": str}]')
def _get_issue(args, state):
    _maybe_fail('get_issue', args['key'], state)
    i = ISSUE_BY_KEY.get(args['key'])
    if i is None:
        raise _not_found(f'issue {args["key"]}')
    return i


@tool('add_comment', 'Add a comment to an issue.', {'key': ('string', 'Issue key'), 'body': ('string', 'Comment text')},
      'an object {"ok": true}')
def _add_comment(args, state):
    if args['key'] not in ISSUE_BY_KEY:
        raise _not_found(f'issue {args["key"]}')
    return {'ok': True}


@tool('update_issue', 'Update fields of an issue.',
      {'key': ('string', 'Issue key'), 'fields': ('object', 'Fields to set: status, priority, assignee, labels')},
      'the updated issue object (without comments)')
def _update_issue(args, state):
    i = ISSUE_BY_KEY.get(args['key'])
    if i is None:
        raise _not_found(f'issue {args["key"]}')
    bad = set(args['fields']) - {'status', 'priority', 'assignee', 'labels'}
    if bad:
        raise ToolError(f'cannot update {sorted(bad)}', 'ValueError')
    return {**{k: v for k, v in i.items() if k != 'comments'}, **args['fields']}


@tool('list_team', 'List the support team with current on-call status.', {},
      'a list of {"user": str, "name": str, "on_call": bool, "timezone": str}', required=[])
def _list_team(args, state):
    return [{'user': u, 'name': n, 'on_call': u in ('bjorn', 'emi'), 'timezone': USERS_TZ[u][1]} for u, n in TEAM.items()]


# ==========================================================================
# External services: weather, quotes, health checks (parallel and retry tasks)

CITIES = {'London': (11.2, 'cloudy', 81), 'Paris': (13.5, 'rain', 77), 'Berlin': (9.8, 'cloudy', 70),
          'Madrid': (19.4, 'sunny', 41), 'Rome': (17.1, 'sunny', 55), 'Oslo': (2.3, 'snow', 88),
          'Tokyo': (14.9, 'clear', 60), 'Sydney': (24.6, 'sunny', 52), 'New York': (8.1, 'windy', 63),
          'Chicago': (4.4, 'rain', 72), 'Austin': (23.8, 'sunny', 45), 'Toronto': (1.9, 'snow', 79)}


@tool('get_weather', 'Current weather for a city.', {'city': ('string', 'City name')},
      'an object {"city": str, "temp_c": float, "condition": str, "humidity": int percent}')
def _get_weather(args, state):
    _maybe_fail('get_weather', args['city'], state)
    w = CITIES.get(args['city'])
    if w is None:
        raise _not_found(f'city {args["city"]}')
    return {'city': args['city'], 'temp_c': w[0], 'condition': w[1], 'humidity': w[2]}


QUOTES = {'ACME': (182.35, 1.24), 'GLBX': (47.02, -2.81), 'INIT': (310.9, 0.05), 'UMBR': (12.77, -0.66),
          'WAYN': (95.5, 3.9), 'STRK': (221.13, -1.02), 'OSCP': (64.48, 0.0), 'CYDN': (8.04, 12.5)}


@tool('get_quote', 'Latest stock quote.', {'symbol': ('string', 'Ticker symbol')},
      'an object {"symbol": str, "price": float, "change_pct": float, "currency": "USD"}')
def _get_quote(args, state):
    _maybe_fail('get_quote', args['symbol'], state)
    q = QUOTES.get(args['symbol'])
    if q is None:
        raise _not_found(f'symbol {args["symbol"]}')
    return {'symbol': args['symbol'], 'price': q[0], 'change_pct': q[1], 'currency': 'USD'}


ENDPOINTS = {'https://api.example.com': (True, 42), 'https://auth.example.com': (True, 88),
             'https://billing.example.com': (False, None), 'https://search.example.com': (True, 340),
             'https://cdn.example.com': (True, 12), 'https://mail.example.com': (False, None),
             'https://status.example.com': (True, 51)}


@tool('check_health', 'Probe an endpoint\'s /health.', {'url': ('string', 'Base URL')},
      'an object {"url": str, "up": bool, "latency_ms": int or null}')
def _check_health(args, state):
    _maybe_fail('check_health', args['url'], state)
    e = ENDPOINTS.get(args['url'])
    if e is None:
        raise ToolError(f'unknown endpoint {args["url"]}', 'ValueError')
    return {'url': args['url'], 'up': e[0], 'latency_ms': e[1]}


def _accounts():
    rnd = random.Random(1357)
    accts = []
    for i in range(1, 46):
        accts.append({'account_id': f'A{i:03d}', 'owner': CUSTOMERS[i % 40]['name'],
                      'balance': round(rnd.uniform(-500, 25000), 2), 'currency': rnd.choice(['USD', 'USD', 'EUR', 'GBP']),
                      'status': rnd.choices(['active', 'frozen', 'closed'], [80, 10, 10])[0],
                      'opened': (dt.date(2020, 1, 1) + dt.timedelta(days=rnd.randrange(2200))).isoformat()})
    return accts


ACCOUNTS = _accounts()


@tool('list_accounts', 'List bank accounts, 10 per page. Pass the offset of the next page to continue.',
      {'offset': ('integer', 'Index of the first account to return (default 0)'),
       'limit': ('integer', 'Page size, at most 10 (default 10)')},
      'an object {"accounts": [{"account_id": str, "owner": str, "balance": float, "currency": str, '
      '"status": "active"|"frozen"|"closed", "opened": "YYYY-MM-DD"}], "total": int}', required=[])
def _list_accounts(args, state):
    off = args.get('offset', 0)
    lim = min(args.get('limit', 10), 10)
    _maybe_fail('list_accounts', off, state)
    return {'accounts': ACCOUNTS[off:off + lim], 'total': len(ACCOUNTS)}


def _transactions():
    rnd = random.Random(8642)
    out = {}
    merchants = ['Grocer', 'CloudHost', 'Airline', 'Cafe', 'Bookshop', 'Utility Co', 'Payroll', 'Refund']
    for a in ACCOUNTS[:12]:
        rows = []
        t = dt.datetime(2026, 2, 1)
        for k in range(rnd.randrange(8, 20)):
            t += dt.timedelta(hours=rnd.randrange(5, 90))
            m = rnd.choice(merchants)
            amt = round(rnd.uniform(3, 400), 2) * (1 if m in ('Payroll', 'Refund') else -1)
            rows.append({'id': f'{a["account_id"]}-T{k + 1:02d}', 'timestamp': t.isoformat() + 'Z',
                         'amount': amt, 'merchant': m})
        out[a['account_id']] = rows
    return out


TRANSACTIONS = _transactions()


@tool('get_transactions', 'Transactions of one account since 2026-02-01, oldest first. Negative amounts are debits.',
      {'account_id': ('string', 'Account id like "A007"')},
      'a list of {"id": str, "timestamp": ISO str, "amount": float, "merchant": str}')
def _get_transactions(args, state):
    _maybe_fail('get_transactions', args['account_id'], state)
    if args['account_id'] not in TRANSACTIONS:
        raise _not_found(f'transactions for {args["account_id"]}')
    return TRANSACTIONS[args['account_id']]


@tool('get_job_status', 'Status of an asynchronous export job.', {'job_id': ('string', 'Job id')},
      'an object {"job_id": str, "state": "queued"|"running"|"done"|"failed", "progress": int percent, '
      '"result_url": str or null}')
def _get_job_status(args, state):
    n = state.setdefault('job_polls', {}).get(args['job_id'], 0) + 1
    state['job_polls'][args['job_id']] = n
    plan = state['config'].get('jobs', {}).get(args['job_id'])
    if plan is None:
        raise _not_found(f'job {args["job_id"]}')
    st = plan[min(n, len(plan)) - 1]
    return {'job_id': args['job_id'], 'state': st, 'progress': min(100, n * 100 // len(plan)) if st != 'failed' else 0,
            'result_url': f'https://api.example.com/exports/{args["job_id"]}.csv' if st == 'done' else None}


@tool('start_export', 'Start an asynchronous export of a dataset.', {'dataset': ('string', 'Dataset name')},
      'an object {"job_id": str}')
def _start_export(args, state):
    jobs = state['config'].get('exports', {})
    if args['dataset'] not in jobs:
        raise ToolError(f'unknown dataset {args["dataset"]!r}', 'ValueError')
    return {'job_id': jobs[args['dataset']]}
