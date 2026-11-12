import asyncio, json
from collections import defaultdict

orders = [json.loads(r) for r in await asyncio.gather(
    *(get_order({"order_id": oid}) for oid in range(1100, 1110)))]

product_ids = sorted({it["product_id"] for o in orders for it in o["items"]})
products = [json.loads(r) for r in await asyncio.gather(
    *(get_product({"product_id": pid}) for pid in product_ids))]
category = {p["id"]: p["category"] for p in products}

units = defaultdict(int)
for o in orders:
    for it in o["items"]:
        units[category[it["product_id"]]] += it["qty"]
units = dict(sorted(units.items()))
print(units)
print(await submit_answer({"units": units}))
