from collections import Counter

counts = Counter()
cursor = None
while True:
    args = {"since": "2026-02-01", "until": "2026-02-28"}
    if cursor is not None:
        args["cursor"] = cursor
    page = await list_orders(args)
    counts.update(order["status"] for order in page["orders"])
    cursor = page["next_cursor"]
    if cursor is None:
        break
await submit_answer({"counts": dict(counts)})
