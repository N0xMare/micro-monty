import asyncio, json
from collections import defaultdict
async def customers():
    first = json.loads(await list_customers({"page": 1}))
    rest = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))
    return first["customers"] + [c for raw in rest for c in json.loads(raw)["customers"]]
async def orders():
    result, args = [], {"status": "completed"}
    while True:
        page = json.loads(await list_orders(args))
        result.extend(page["orders"])
        if not page["next_cursor"]:
            return result
        args["cursor"] = page["next_cursor"]
cs, os = await asyncio.gather(customers(), orders())
tiers = {c["id"]: c["tier"] for c in cs}
totals, counts = defaultdict(float), defaultdict(int)
for o in os:
    tier = tiers[o["customer_id"]]
    totals[tier] += o["total"]
    counts[tier] += 1
await submit_answer({"averages": {t: round(totals[t] / counts[t], 2) if counts[t] else 0.0 for t in ("bronze", "silver", "gold")}})
