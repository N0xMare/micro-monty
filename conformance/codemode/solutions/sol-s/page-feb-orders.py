from collections import Counter

args = {"since": "2026-02-01", "until": "2026-02-28"}
counts = Counter()
while True:
    result = await list_orders(args)
    counts.update(order["status"] for order in result["orders"])
    if result["next_cursor"] is None:
        break
    args["cursor"] = result["next_cursor"]
await submit_answer({"counts": dict(counts)})
