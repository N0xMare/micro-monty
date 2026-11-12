from collections import Counter
counts = Counter()
seen = set()
cursor = None
while True:
    args = {'since': '2026-02-01', 'until': '2026-02-28'}
    if cursor:
        args['cursor'] = cursor
    resp = list_orders(args)
    for o in resp['orders']:
        if o['order_date'].startswith('2026-02') and o['id'] not in seen:
            seen.add(o['id'])
            counts[o['status']] += 1
    cursor = resp.get('next_cursor')
    if not cursor:
        break
print(dict(counts))
submit_answer({'counts': dict(counts)})
