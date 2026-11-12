import json

customer = json.loads(await get_customer({"customer_id": 17}))
orders, page = [], json.loads(await list_orders({"customer_id": 17}))
orders.extend(page["orders"])
while page["next_cursor"]:
    page = json.loads(await list_orders({"customer_id": 17, "cursor": page["next_cursor"]}))
    orders.extend(page["orders"])
counts = {}
for order in orders:
    counts[order["status"]] = counts.get(order["status"], 0) + 1
total = round(sum(o["total"] for o in orders if o["status"] == "completed"), 2)
await submit_answer({"name": customer["name"], "counts": counts, "completed_total": total})
