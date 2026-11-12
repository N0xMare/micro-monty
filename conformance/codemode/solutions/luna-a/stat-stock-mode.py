import json
from collections import Counter

products = json.loads(await list_products())
stocks = [int(p["stock"]) for p in products]
counts = Counter(stocks)
mode = min(counts, key=lambda x: (-counts[x], x))
share = 100 * sum(value >= 20 for value in stocks) / len(stocks) if stocks else 0.0
await submit_answer({"mode": mode, "out_of_stock": sum(value == 0 for value in stocks), "share_20_plus": round(share, 1)})
