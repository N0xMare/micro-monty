import json
from collections import Counter

counts = Counter()
cursor = None
while True:
    args = {"since": "2026-02-01", "until": "2026-02-28"}
    if cursor:
        args["cursor"] = cursor
    page = json.loads(await list_orders(args))
    for o in page["orders"]:
        if o["order_date"].startswith("2026-02"):
            counts[o["status"]] += 1
    cursor = page.get("next_cursor")
    if not cursor:
        break

counts = dict(counts)
print(counts)
print(await submit_answer({"counts": counts}))
