import json
from collections import Counter

args = {"since": "2026-02-01", "until": "2026-02-28"}
counts = Counter()
while True:
    page = json.loads(await list_orders(args))
    counts.update(order["status"] for order in page["orders"])
    if page["next_cursor"] is None:
        break
    args = {"since": "2026-02-01", "until": "2026-02-28", "cursor": page["next_cursor"]}
await submit_answer({"counts": dict(counts)})
