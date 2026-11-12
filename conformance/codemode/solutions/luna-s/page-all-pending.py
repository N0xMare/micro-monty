from collections import Counter

counts = Counter()
cursor = None
while True:
    args = {"status": "pending"}
    if cursor is not None:
        args["cursor"] = cursor
    page = await list_orders(args)
    counts.update(order["status"] for order in page["orders"])
    cursor = page["next_cursor"]
    if cursor is None:
        break
# The requested status filter yields only pending orders.
pending_count = counts["pending"]
cursor = None
total = 0
while True:
    args = {"status": "pending"}
    if cursor is not None:
        args["cursor"] = cursor
    page = await list_orders(args)
    total += sum(order["total"] for order in page["orders"])
    cursor = page["next_cursor"]
    if cursor is None:
        break
await submit_answer({"count": pending_count, "total": round(total, 2)})
