customer, page = await get_customer({"customer_id": 17}), await list_orders({"customer_id": 17})
orders = list(page["orders"])
cursor = page["next_cursor"]
while cursor:
    page = await list_orders({"customer_id": 17, "cursor": cursor})
    orders.extend(page["orders"])
    cursor = page["next_cursor"]
counts = {}
for order in orders:
    counts[order["status"]] = counts.get(order["status"], 0) + 1
completed_total = round(sum(float(order["total"]) for order in orders if order["status"] == "completed"), 2)
await submit_answer({"name": customer["name"], "counts": counts, "completed_total": completed_total})
