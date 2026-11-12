import json

orders = []
cursor = None
while True:
    args = {"status": "pending"}
    if cursor:
        args["cursor"] = cursor
    res = json.loads(await list_orders(args))
    orders.extend(res["orders"])
    cursor = res.get("next_cursor")
    if not cursor:
        break
unique = {o["id"]: o for o in orders}
pending = [o for o in unique.values() if o["status"] == "pending"]
total = round(sum(o["total"] for o in pending), 2)
print(len(pending), total)
await submit_answer({"count": len(pending), "total": total})
