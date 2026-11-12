orders = []
cursor = None
while True:
    args = {"status": "pending"}
    if cursor:
        args["cursor"] = cursor
    resp = await list_orders(args)
    orders.extend(resp["orders"])
    cursor = resp.get("next_cursor")
    if not cursor:
        break
# dedupe by id just in case
by_id = {o["id"]: o for o in orders if o["status"] == "pending"}
count = len(by_id)
total = round(sum(o["total"] for o in by_id.values()), 2)
print(count, total)
await submit_answer({"count": count, "total": total})
