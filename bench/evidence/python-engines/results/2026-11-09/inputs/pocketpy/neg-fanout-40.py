pass
ids = range(1001, 1041)
orders = [*(get_order({'order_id': i}) for i in ids)]
total = round(sum((o['total'] for o in orders)), 2)
refunded = sum((1 for o in orders if str(o.get('status', '')).lower() == 'refunded'))
currencies = {o.get('currency') for o in orders}
print(f'orders={len(orders)} total={total} refunded={refunded} currencies={currencies}')
print(submit_answer({'total': total, 'refunded': refunded}))
