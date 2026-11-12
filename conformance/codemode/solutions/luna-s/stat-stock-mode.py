from collections import Counter

products = await list_products({})
stocks = [product["stock"] for product in products]
frequencies = Counter(stocks)
mode = min(frequencies, key=lambda value: (-frequencies[value], value))
share = sum(value >= 20 for value in stocks) / len(stocks) * 100 if stocks else 0.0
await submit_answer({"mode": mode, "out_of_stock": sum(value == 0 for value in stocks), "share_20_plus": round(share, 1)})
