import asyncio, json

order = json.loads(await get_order({"order_id": 1042}))
pids = sorted({it["product_id"] for it in order["items"]})
prods = {p["id"]: p for p in (json.loads(s) for s in await asyncio.gather(*[get_product({"product_id": pid}) for pid in pids]))}

subtotals = {}
for it in order["items"]:
    cat = prods[it["product_id"]]["category"]
    subtotals[cat] = subtotals.get(cat, 0.0) + it["qty"] * it["unit_price"]
subtotals = {k: round(v, 2) for k, v in subtotals.items()}
print(order["total"], subtotals)
print(await submit_answer({"subtotals": subtotals}))
