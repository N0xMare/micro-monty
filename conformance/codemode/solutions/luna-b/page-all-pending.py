import json

orders = []
cursor = None
while True:
    arguments = {"status": "pending"}
    if cursor is not None:
        arguments["cursor"] = cursor
    page = json.loads(await list_orders(arguments))
    orders.extend(page["orders"])
    cursor = page["next_cursor"]
    if cursor is None:
        break
await submit_answer({"count": len(orders), "total": round(sum(order["total"] for order in orders), 2)})
