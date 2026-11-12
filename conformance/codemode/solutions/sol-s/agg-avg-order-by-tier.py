import asyncio
from collections import defaultdict
from decimal import Decimal

first = await list_customers({"page": 1})
pages = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))
tiers = {c["id"]: c["tier"] for page in [first, *pages] for c in page["customers"]}
sums = defaultdict(Decimal)
counts = defaultdict(int)
args = {"status": "completed"}
while True:
    page = await list_orders(args)
    for order in page["orders"]:
        tier = tiers[order["customer_id"]]
        sums[tier] += Decimal(str(order["total"]))
        counts[tier] += 1
    if page["next_cursor"] is None:
        break
    args = {"status": "completed", "cursor": page["next_cursor"]}
await submit_answer({"averages": {t: float(round(sums[t] / counts[t], 2)) if counts[t] else 0.0 for t in ("bronze", "silver", "gold")}})
