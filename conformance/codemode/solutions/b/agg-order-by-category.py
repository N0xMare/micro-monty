import asyncio, json

order = json.loads(await get_order({"order_id": 1042}))
print(order)
pids = sorted({it["product_id"] for it in order["items"]})
prods = [json.loads(p) for p in await asyncio.gather(*(get_product({"product_id": p}) for p in pids))]
cat_of = {p["id"]: p["category"] for p in prods}

subtotals = {}
for it in order["items"]:
    c = cat_of[it["product_id"]]
    subtotals[c] = subtotals.get(c, 0.0) + it["qty"] * it["unit_price"]
subtotals = {c: round(v, 2) for c, v in subtotals.items()}
print(subtotals)
print(await submit_answer({"subtotals": subtotals}))
