import asyncio, json
from collections import defaultdict

order_ids = list(range(1100, 1110))
orders = [json.loads(s) for s in await asyncio.gather(*(get_order({"order_id": i}) for i in order_ids))]
product_ids = sorted({item["product_id"] for order in orders for item in order["items"]})
products = await asyncio.gather(*(get_product({"product_id": pid}) for pid in product_ids))
category_by_id = {p["id"]: p["category"] for p in map(json.loads, products)}
units = defaultdict(int)
for order in orders:
    for item in order["items"]:
        units[category_by_id[item["product_id"]]] += item["qty"]
await submit_answer({"units": dict(units)})
