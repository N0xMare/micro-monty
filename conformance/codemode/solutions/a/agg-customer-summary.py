import json
from collections import Counter

customer = json.loads(await get_customer({"customer_id": 17}))

orders = []
cursor = None
while True:
    args = {"customer_id": 17}
    if cursor:
        args["cursor"] = cursor
    res = json.loads(await list_orders(args))
    orders.extend(res["orders"])
    cursor = res.get("next_cursor")
    if not cursor:
        break

orders = [o for o in orders if o["customer_id"] == 17]
counts = dict(Counter(o["status"] for o in orders))
completed_total = round(sum(o["total"] for o in orders if o["status"] == "completed"), 2)
print(customer["name"], counts, completed_total)
print(await submit_answer({"name": customer["name"], "counts": counts, "completed_total": completed_total}))
