import asyncio
from decimal import Decimal

ids = [f"P{i:02d}" for i in range(1, 13)]
products = await asyncio.gather(*(get_product({"product_id": product_id}) for product_id in ids))
values = {product_id: Decimal(str(product["price"])) * product["stock"] for product_id, product in zip(ids, products)}
await submit_answer({"total": float(round(sum(values.values()), 2)), "top_product": max(values, key=values.get)})
