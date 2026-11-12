import asyncio
import json

product_ids = [f"P{i:02d}" for i in range(1, 13)]
results = await asyncio.gather(*(get_product({"product_id": product_id}) for product_id in product_ids))
values = {product_id: (product := json.loads(result))["price"] * product["stock"] for product_id, result in zip(product_ids, results)}
await submit_answer({"total": round(sum(values.values()), 2), "top_product": max(values, key=values.get)})
