import json

orders = []
cursor = None
while True:
    args = {"status": "pending"}
    if cursor:
        args["cursor"] = cursor
    page = json.loads(await list_orders(args))
    orders.extend(page["orders"])
    cursor = page.get("next_cursor")
    if not cursor:
        break

pending = [o for o in orders if o["status"] == "pending"]
count = len(pending)
total = round(sum(o["total"] for o in pending), 2)
print(count, total)
print(await submit_answer({"count": count, "total": total}))
