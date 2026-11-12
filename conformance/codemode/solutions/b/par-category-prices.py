import asyncio, json, statistics

cats = ["hardware", "software", "accessories", "services"]
results = await asyncio.gather(*(list_products({"category": c}) for c in cats))
medians = {}
for c, r in zip(cats, results):
    prices = [p["price"] for p in json.loads(r) if p["category"] == c]
    medians[c] = round(statistics.median(prices), 2)
print(medians)
print(await submit_answer({"medians": medians}))
