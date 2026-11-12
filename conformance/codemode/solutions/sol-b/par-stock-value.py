import asyncio
import json
from decimal import Decimal

ids = [f"P{number:02d}" for number in range(1, 13)]
raw = await asyncio.gather(*(get_product({"product_id": product_id}) for product_id in ids))
values = {}
for product_id, result in zip(ids, raw):
    product = json.loads(result)
    values[product_id] = Decimal(str(product["price"])) * product["stock"]
await submit_answer({"total": float(round(sum(values.values()), 2)), "top_product": max(values, key=values.get)})
