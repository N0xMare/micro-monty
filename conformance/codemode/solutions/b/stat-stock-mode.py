import json
from collections import Counter

products = json.loads(await list_products({}))
stocks = [int(p["stock"]) for p in products]
counts = Counter(stocks)
best = max(counts.values())
mode = min(s for s, c in counts.items() if c == best)
out_of_stock = sum(1 for s in stocks if s <= 0)
share = round(100 * sum(1 for s in stocks if s >= 20) / len(stocks), 1)
result = {"mode": mode, "out_of_stock": out_of_stock, "share_20_plus": share}
print(len(stocks), result)
print(await submit_answer(result))
