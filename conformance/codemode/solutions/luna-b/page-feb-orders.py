import json
from collections import Counter

counts = Counter()
cursor = None
while True:
    arguments = {"since": "2026-02-01", "until": "2026-02-28"}
    if cursor is not None:
        arguments["cursor"] = cursor
    page = json.loads(await list_orders(arguments))
    counts.update(order["status"] for order in page["orders"])
    cursor = page["next_cursor"]
    if cursor is None:
        break
await submit_answer({"counts": dict(counts)})
