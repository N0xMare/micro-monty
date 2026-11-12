import asyncio, json

ids = [f"P{n:02d}" for n in range(1, 13)]
results = await asyncio.gather(*(get_product({"product_id": p}) for p in ids))
values = {}
for p, r in zip(ids, results):
    prod = json.loads(r)
    values[p] = prod["price"] * prod["stock"]
total = round(sum(values.values()), 2)
top = max(values, key=values.get)
print(values)
print(await submit_answer({"total": total, "top_product": top}))
