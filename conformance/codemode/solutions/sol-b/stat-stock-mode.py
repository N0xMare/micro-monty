import json
from collections import Counter
products = json.loads(await list_products({}))
stock = [product["stock"] for product in products]
counts = Counter(stock)
mode = min(counts, key=lambda value: (-counts[value], value))
await submit_answer({"mode": mode, "out_of_stock": counts[0], "share_20_plus": round(100 * sum(value >= 20 for value in stock) / len(stock), 1)})
