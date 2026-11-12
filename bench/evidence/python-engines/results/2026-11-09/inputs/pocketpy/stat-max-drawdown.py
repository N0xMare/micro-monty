pass
ids = [f'A{i:03d}' for i in range(1, 13)]
results = [*[get_transactions({'account_id': a}) for a in ids]]
lows = {}
for a, txs in zip(ids, results):
    if isinstance(txs, Exception):
        print(a, 'error:', txs)
        continue
    running = 0.0
    low = 0.0
    for t in txs:
        running += t['amount']
        low = min(low, running)
    lows[a] = low
    print(a, len(txs), round(low, 2))
deepest = min(lows, key=lows.get)
submit_answer({'account_id': deepest, 'low': round(lows[deepest], 2)})
