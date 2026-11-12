"""API/DB aggregation over the shop world."""

from collections import Counter, defaultdict

from ptc import r2, task
from tasks.worlds import CUSTOMER_BY_ID, CUSTOMERS, FX, ORDER_ITEMS, ORDERS, PRODUCT_BY_ID, PRODUCTS, REGIONS

C = 'api-db-aggregation'
MONEY = 0.006
done = [o for o in ORDERS if o['status'] == 'completed']


def _region_totals(month):
    t = defaultdict(float)
    for o in done:
        if o['order_date'].startswith(month):
            t[CUSTOMER_BY_ID[o['customer_id']]['region']] += o['total']
    return {r: r2(v) for r, v in t.items()}


task('agg-revenue-by-region', C, """
Using the shop database, compute the total revenue of completed orders placed in February 2026,
grouped by the customer's region.
""", ['query_database'],
    answer={'totals': ('object', 'Region name -> total revenue in USD, rounded to 2 decimals. Only regions with revenue.')},
    expect={'totals': _region_totals('2026-02')}, tol=MONEY)

spend = defaultdict(float)
for o in done:
    spend[o['customer_id']] += o['total']
top5 = sorted(spend.items(), key=lambda kv: -kv[1])[:5]
task('agg-top-customers', C, """
Find the five customers who spent the most on completed orders (all time).
""", ['query_database'],
    answer={'top': ('array', 'List of {"name": str, "total": number rounded to 2 decimals}, highest total first')},
    expect={'top': [{'name': CUSTOMER_BY_ID[c]['name'], 'total': r2(v)} for c, v in top5]}, tol=MONEY)

tier_tot = defaultdict(list)
for o in done:
    tier_tot[CUSTOMER_BY_ID[o['customer_id']]['tier']].append(o['total'])
task('agg-avg-order-by-tier', C, """
What is the average total of a completed order for each customer tier (bronze, silver, gold)?
Use the REST tools (there is no SQL access for this task).
""", ['list_customers', 'list_orders'],
    answer={'averages': ('object', 'Tier -> average completed order total, rounded to 2 decimals')},
    expect={'averages': {t: r2(sum(v) / len(v)) for t, v in tier_tot.items()}}, tol=MONEY)

c17 = [o for o in ORDERS if o['customer_id'] == 17]
task('agg-customer-summary', C, """
Summarize customer 17: their name, how many orders they have in each status (only statuses that
occur), and the total they spent on completed orders.
""", ['get_customer', 'list_orders'],
    answer={'name': 'Customer name', 'counts': ('object', 'Status -> number of orders'),
            'completed_total': ('number', 'Sum of completed order totals, rounded to 2 decimals')},
    expect={'name': CUSTOMER_BY_ID[17]['name'], 'counts': dict(Counter(o['status'] for o in c17)),
            'completed_total': r2(sum(o['total'] for o in c17 if o['status'] == 'completed'))}, tol=MONEY)

q1_ids = {o['id'] for o in done if '2026-01-01' <= o['order_date'] <= '2026-03-31'}
units = Counter()
for i in ORDER_ITEMS:
    if i['order_id'] in q1_ids:
        units[PRODUCT_BY_ID[i['product_id']]['category']] += i['qty']
best_cat = units.most_common(1)[0]
task('agg-best-category-q1', C, """
Which product category sold the most units (sum of quantities) in completed orders placed in
Q1 2026 (January 1 to March 31)?
""", ['query_database'],
    answer={'category': 'The category name', 'units': ('integer', 'Units sold in that category')},
    expect={'category': best_cat[0], 'units': best_cat[1]})

gold = Counter(c['region'] for c in CUSTOMERS if c['tier'] == 'gold')
task('agg-gold-by-region', C, """
How many gold-tier customers are there in each region? Report all four regions (North, South, East,
West), using 0 where there are none.
""", ['list_customers'],
    answer={'counts': ('object', 'Region -> number of gold customers')},
    expect={'counts': {r: gold.get(r, 0) for r in REGIONS}})

o42 = [i for i in ORDER_ITEMS if i['order_id'] == 1042]
cat42 = defaultdict(float)
for i in o42:
    cat42[PRODUCT_BY_ID[i['product_id']]['category']] += i['qty'] * i['unit_price']
task('agg-order-by-category', C, """
Break down order 1042 by product category: for each category present in the order, the subtotal
(quantity x unit price).
""", ['get_order', 'get_product'],
    answer={'subtotals': ('object', 'Category -> subtotal rounded to 2 decimals')},
    expect={'subtotals': {k: r2(v) for k, v in cat42.items()}}, tol=MONEY)

mar = sum(o['total'] for o in done if o['order_date'].startswith('2026-03'))
task('agg-march-eur', C, """
What is the total of completed orders placed in March 2026, converted from USD to EUR at the current
exchange rate?
""", ['query_database', 'get_exchange_rate'],
    answer={'total_usd': ('number', 'Total in USD, rounded to 2 decimals'),
            'total_eur': ('number', 'Total in EUR, rounded to 2 decimals')},
    expect={'total_usd': r2(mar), 'total_eur': r2(mar * FX[('USD', 'EUR')])}, tol=0.02)

cnt = Counter(o['customer_id'] for o in done)
rep = sorted(c for c, n in cnt.items() if n >= 3)
task('agg-repeat-customers', C, """
Which customers have placed at least three completed orders?
""", ['query_database'],
    answer={'count': ('integer', 'How many such customers'),
            'customer_ids': ('array', 'Their ids, ascending')},
    expect={'count': len(rep), 'customer_ids': rep})

sold = Counter()
done_ids = {o['id'] for o in done}
for i in ORDER_ITEMS:
    if i['order_id'] in done_ids:
        sold[i['product_id']] += i['qty']
low = sorted(p['id'] for p in PRODUCTS if p['stock'] < 10 and sold[p['id']] >= 5)
task('agg-low-stock-sellers', C, """
Find products that are low on stock (fewer than 10 units in stock) but sold at least 5 units across
all completed orders. These need restocking.
""", ['list_products', 'query_database'],
    answer={'product_ids': ('array', 'Product ids, ascending')},
    expect={'product_ids': low})

months = ['2025-10', '2025-11', '2025-12', '2026-01', '2026-02', '2026-03']
task('agg-monthly-trend', C, """
Report the monthly revenue of completed orders from October 2025 through March 2026.
""", ['query_database'],
    answer={'months': ('array', 'List of {"month": "YYYY-MM", "total": number rounded to 2 decimals}, oldest first, '
                       'one entry per month')},
    expect={'months': [{'month': m, 'total': r2(sum(o['total'] for o in done if o['order_date'].startswith(m)))}
                       for m in months]}, tol=MONEY)

reg_all, reg_can = Counter(), Counter()
for o in ORDERS:
    r = CUSTOMER_BY_ID[o['customer_id']]['region']
    reg_all[r] += 1
    reg_can[r] += o['status'] == 'cancelled'
task('agg-cancel-rate', C, """
For each region, what percentage of all orders were cancelled?
""", ['query_database'],
    answer={'rates': ('object', 'Region -> cancelled orders / all orders * 100, rounded to 1 decimal')},
    expect={'rates': {r: round(reg_can[r] / reg_all[r] * 100, 1) for r in REGIONS}}, tol=0.06)

ordered = {o['customer_id'] for o in ORDERS}
never = sorted(c['id'] for c in CUSTOMERS if c['signup_date'].startswith('2025') and c['id'] not in ordered)
task('agg-signups-no-orders', C, """
Which customers signed up during 2025 but have never placed any order (in any status)?
""", ['query_database'],
    answer={'customer_ids': ('array', 'Customer ids, ascending (empty list if none)')},
    expect={'customer_ids': never})

big = max(done, key=lambda o: o['total'])
big_names = sorted({PRODUCT_BY_ID[i['product_id']]['name'] for i in ORDER_ITEMS if i['order_id'] == big['id']})
task('agg-largest-order', C, """
Find the largest completed order by total. Report its id, the customer's name, and the names of the
products in it.
""", ['list_orders', 'get_order', 'get_customer', 'get_product'],
    answer={'order_id': ('integer', 'Order id'), 'customer': 'Customer name',
            'products': ('array', 'Distinct product names, sorted alphabetically')},
    expect={'order_id': big['id'], 'customer': CUSTOMER_BY_ID[big['customer_id']]['name'], 'products': big_names})

qty = defaultdict(int)
for i in ORDER_ITEMS:
    if i['order_id'] in done_ids:
        qty[i['order_id']] += i['qty']
task('agg-avg-units', C, """
On average, how many units (sum of item quantities) does a completed order contain?
""", ['query_database'],
    answer={'average_units': ('number', 'Rounded to 2 decimals')},
    expect={'average_units': r2(sum(qty.values()) / len(done))}, tol=MONEY)

gb = sum(o['total'] for o in done if CUSTOMER_BY_ID[o['customer_id']]['country'] == 'GB')
task('agg-gb-revenue-gbp', C, """
How much revenue (completed orders, all time) came from customers located in Great Britain (country
code GB)? Report it in GBP.
""", ['query_database', 'get_exchange_rate'],
    answer={'total_gbp': ('number', 'Revenue converted from USD to GBP, rounded to 2 decimals')},
    expect={'total_gbp': r2(gb * FX[('USD', 'GBP')])}, tol=0.02)

up = sorted(((c, s) for c, s in spend.items() if CUSTOMER_BY_ID[c]['tier'] == 'bronze' and s > 2000),
            key=lambda kv: -kv[1])
task('agg-upgrade-candidates', C, """
Bronze-tier customers who spent more than $2,000 on completed orders are candidates for an upgrade.
List them.
""", ['query_database'],
    answer={'candidates': ('array', 'List of {"id": int, "name": str, "spend": number rounded to 2 decimals}, '
                           'highest spend first')},
    expect={'candidates': [{'id': c, 'name': CUSTOMER_BY_ID[c]['name'], 'spend': r2(s)} for c, s in up]}, tol=MONEY)
