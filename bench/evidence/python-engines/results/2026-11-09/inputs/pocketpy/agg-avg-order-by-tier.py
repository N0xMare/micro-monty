pass

def all_customers():
    first = list_customers({'page': 1})
    customers = list(first['customers'])
    pages = first.get('total_pages', 1)
    if pages > 1:
        rest = [*(list_customers({'page': p}) for p in range(2, pages + 1))]
        for r in rest:
            customers.extend(r['customers'])
    return customers

def all_completed_orders():
    orders = []
    cursor = None
    while True:
        args = {'status': 'completed'}
        if cursor:
            args['cursor'] = cursor
        res = list_orders(args)
        orders.extend(res['orders'])
        cursor = res.get('next_cursor')
        if not cursor:
            break
    return orders
customers, orders = [all_customers(), all_completed_orders()]
tier_of = {c['id']: c['tier'] for c in customers}
seen = {}
for o in orders:
    seen[o['id']] = o
sums = {}
counts = {}
for o in seen.values():
    if o.get('status') != 'completed':
        continue
    t = tier_of.get(o['customer_id'])
    if t is None:
        continue
    sums[t] = sums.get(t, 0.0) + o['total']
    counts[t] = counts.get(t, 0) + 1
averages = {t: round(sums[t] / counts[t], 2) for t in ('bronze', 'silver', 'gold') if counts.get(t)}
print(averages, counts)
print(submit_answer({'averages': averages}))
