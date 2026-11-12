import statistics

categories = ["hardware", "software", "accessories", "services"]
results = await asyncio.gather(*(list_products({"category": c}) for c in categories))
medians = {}
for c, products in zip(categories, results):
    prices = [p["price"] for p in products if p["category"] == c]
    print(c, len(prices))
    if prices:
        medians[c] = round(statistics.median(prices), 2)
print(medians)
await submit_answer({"medians": medians})
