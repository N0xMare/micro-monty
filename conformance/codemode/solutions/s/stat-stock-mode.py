from collections import Counter

products = await list_products({})
stocks = [int(p["stock"]) for p in products]
counts = Counter(stocks)
top = max(counts.values())
mode = min(s for s, c in counts.items() if c == top)
out_of_stock = sum(1 for s in stocks if s == 0)
share = round(100 * sum(1 for s in stocks if s >= 20) / len(stocks), 1)
print(len(stocks), mode, out_of_stock, share)
await submit_answer({"mode": mode, "out_of_stock": out_of_stock, "share_20_plus": share})
