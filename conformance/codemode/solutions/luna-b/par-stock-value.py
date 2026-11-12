import asyncio
import json

product_ids = [f"P{i:02d}" for i in range(1, 13)]
responses = await asyncio.gather(*(get_product({"product_id": product_id}) for product_id in product_ids))
products = [json.loads(response) for response in responses]
values = {product["id"]: product["price"] * product["stock"] for product in products}
top_product = min(values, key=lambda product_id: (-values[product_id], product_id))
await submit_answer({"total": round(sum(values.values()), 2), "top_product": top_product})
