import asyncio

product_ids = [f"P{i:02d}" for i in range(1, 13)]
products = await asyncio.gather(*(get_product({"product_id": product_id}) for product_id in product_ids))
values = {product["id"]: product["price"] * product["stock"] for product in products}
await submit_answer({"total": round(sum(values.values()), 2), "top_product": max(values, key=values.get)})
