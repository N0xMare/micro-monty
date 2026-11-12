import asyncio
import json
from decimal import Decimal

async def customers():
    first = json.loads(await list_customers({"page": 1}))
    pages = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))
    return first["customers"] + [c for page in pages for c in json.loads(page)["customers"]]

async def orders():
    result = []
    args = {"status": "completed"}
    while True:
        page = json.loads(await list_orders(args))
        result.extend(page["orders"])
        if page["next_cursor"] is None:
            return result
        args["cursor"] = page["next_cursor"]

cs, os = await asyncio.gather(customers(), orders())
tiers = {c["id"]: c["tier"] for c in cs}
totals = {t: Decimal(0) for t in ("bronze", "silver", "gold")}
counts = dict.fromkeys(totals, 0)
for order in os:
    tier = tiers[order["customer_id"]]
    totals[tier] += Decimal(str(order["total"]))
    counts[tier] += 1
await submit_answer({"averages": {t: float(round(totals[t] / counts[t], 2)) if counts[t] else 0 for t in totals}})
