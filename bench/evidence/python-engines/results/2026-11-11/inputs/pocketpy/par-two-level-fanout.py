pass
from collections import defaultdict
orders = [*(get_order({'order_id': oid}) for oid in range(1100, 1110))]
product_ids = sorted({item['product_id'] for o in orders for item in o['items']})
products = [*(get_product({'product_id': pid}) for pid in product_ids)]
category_of = {p['id']: p['category'] for p in products}
units = defaultdict(int)
for o in orders:
    for item in o['items']:
        units[category_of[item['product_id']]] += item['qty']
units = dict(sorted(units.items()))
print(units)
submit_answer({'units': units})
