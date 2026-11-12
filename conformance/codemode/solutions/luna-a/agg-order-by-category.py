import json
o = json.loads(await get_order({"order_id": 1042})); totals = {}
for i in o["items"]:
    p = json.loads(await get_product({"product_id": i["product_id"]}))
    totals[p["category"]] = totals.get(p["category"], 0) + i["qty"] * i["unit_price"]
await submit_answer({"subtotals": {k: round(v, 2) for k, v in totals.items()}})
