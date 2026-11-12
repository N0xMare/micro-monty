ids = [f"P{i:02d}" for i in range(1, 13)]
products = await asyncio.gather(*(get_product({"product_id": p}) for p in ids))
values = {p["id"]: p["price"] * p["stock"] for p in products}
total = round(sum(values.values()), 2)
top = max(values, key=values.get)
print(values, total, top)
await submit_answer({"total": total, "top_product": top})
