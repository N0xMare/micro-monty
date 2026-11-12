import json
from collections import Counter

products = json.loads(await list_products({}))
counts = Counter(p["stock"] for p in products)
mode = min((value for value, count in counts.items() if count == max(counts.values())))
await submit_answer({"mode": mode, "out_of_stock": sum(p["stock"] == 0 for p in products), "share_20_plus": round(100 * sum(p["stock"] >= 20 for p in products) / len(products), 1)})
