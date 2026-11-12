import asyncio
import json
from collections import defaultdict

orders = await asyncio.gather(*(get_order({"order_id": order_id}) for order_id in range(1100, 1110)))
order_data = [json.loads(result) for result in orders]
product_ids = sorted({item["product_id"] for order in order_data for item in order["items"]})
products = await asyncio.gather(*(get_product({"product_id": product_id}) for product_id in product_ids))
categories = {product_id: json.loads(result)["category"] for product_id, result in zip(product_ids, products)}
units = defaultdict(int)
for order in order_data:
    for item in order["items"]:
        units[categories[item["product_id"]]] += item["qty"]
await submit_answer({"units": dict(units)})
