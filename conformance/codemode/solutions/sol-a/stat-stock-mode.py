import json
from collections import Counter
stocks = [product["stock"] for product in json.loads(await list_products({}))]
counts = Counter(stocks)
mode = min(counts, key=lambda value: (-counts[value], value))
await submit_answer({"mode": mode, "out_of_stock": counts[0], "share_20_plus": round(100 * sum(stock >= 20 for stock in stocks) / len(stocks), 1)})
