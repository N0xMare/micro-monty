from collections import Counter

products = await list_products({})
stocks = [p["stock"] for p in products]
counts = Counter(stocks)
mode = min(counts, key=lambda value: (-counts[value], value))
await submit_answer({"mode": mode, "out_of_stock": counts[0], "share_20_plus": round(100 * sum(s >= 20 for s in stocks) / len(stocks), 1)})
