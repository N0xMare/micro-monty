import asyncio

order = await get_order({"order_id": 1042})
items = order["items"]
pids = sorted({it["product_id"] for it in items})
products = await asyncio.gather(*(get_product({"product_id": p}) for p in pids))
category_of = {p["id"]: p["category"] for p in products}

subtotals = {}
for it in items:
    cat = category_of[it["product_id"]]
    subtotals[cat] = subtotals.get(cat, 0.0) + it["qty"] * it["unit_price"]
subtotals = {k: round(v, 2) for k, v in subtotals.items()}
print(order["total"], subtotals)
print(await submit_answer({"subtotals": subtotals}))
