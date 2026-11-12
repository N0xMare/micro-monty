import asyncio, json

orders = [json.loads(s) for s in await asyncio.gather(*(get_order({"order_id": i}) for i in range(1100, 1110)))]
pids = sorted({it["product_id"] for o in orders for it in o["items"]})
products = [json.loads(s) for s in await asyncio.gather(*(get_product({"product_id": p}) for p in pids))]
category = {p["id"]: p["category"] for p in products}

units = {}
for o in orders:
    for it in o["items"]:
        c = category[it["product_id"]]
        units[c] = units.get(c, 0) + it["qty"]

print(units)
print(await submit_answer({"units": units}))
