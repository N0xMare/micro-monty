import asyncio
import json
from decimal import Decimal

ids = [f"P{i:02d}" for i in range(1, 13)]
results = await asyncio.gather(*(get_product({"product_id": product_id}) for product_id in ids))
values = {}
for product_id, result in zip(ids, results):
    product = json.loads(result, parse_float=Decimal)
    values[product_id] = Decimal(str(product["price"])) * product["stock"]
top = min(values, key=lambda product_id: (-values[product_id], product_id))
await submit_answer({"total": float(round(sum(values.values(), Decimal(0)), 2)), "top_product": top})
