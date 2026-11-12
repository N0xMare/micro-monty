import json
from collections import Counter

counts = Counter()
seen = set()
cursor = None
while True:
    args = {"since": "2026-02-01", "until": "2026-02-28"}
    if cursor:
        args["cursor"] = cursor
    res = json.loads(await list_orders(args))
    for o in res["orders"]:
        if o["order_date"].startswith("2026-02") and o["id"] not in seen:
            seen.add(o["id"])
            counts[o["status"]] += 1
    cursor = res.get("next_cursor")
    if not cursor:
        break
print(dict(counts))
await submit_answer({"counts": dict(counts)})
