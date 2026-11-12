import json

cursor = None
count = 0
total = 0.0
while True:
    args = {"status": "pending"}
    if cursor is not None:
        args["cursor"] = cursor
    page = json.loads(await list_orders(args))
    for order in page["orders"]:
        count += 1
        total += order["total"]
    cursor = page["next_cursor"]
    if cursor is None:
        break
await submit_answer({"count": count, "total": round(total, 2)})
