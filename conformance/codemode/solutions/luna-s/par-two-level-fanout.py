import asyncio
orders = await asyncio.gather(*(get_order({"order_id": i}) for i in range(1100, 1110)))
ids = sorted({item["product_id"] for order in orders for item in order["items"]})
products = await asyncio.gather(*(get_product({"product_id": pid}) for pid in ids))
category = {p["id"]: p["category"] for p in products}
units = {}
for order in orders:
    for item in order["items"]:
        key = category[item["product_id"]]
        units[key] = units.get(key, 0) + item["qty"]
await submit_answer({"units": units})
