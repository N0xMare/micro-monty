import asyncio
from collections import Counter

orders = await asyncio.gather(*(get_order({"order_id": i}) for i in range(1100, 1110)))
ids = sorted({item["product_id"] for order in orders for item in order["items"]})
products = await asyncio.gather(*(get_product({"product_id": i}) for i in ids))
categories = {p["id"]: p["category"] for p in products}
units = Counter()
for order in orders:
    for item in order["items"]:
        units[categories[item["product_id"]]] += item["qty"]
await submit_answer({"units": dict(units)})
