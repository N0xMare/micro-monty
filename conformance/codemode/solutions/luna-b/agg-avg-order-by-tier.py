import asyncio, json

first = json.loads(await list_customers({"page": 1}))
customer_pages = [first] + await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))
tiers = {c["id"]: c["tier"] for page in customer_pages for c in page["customers"]}
orders = []
page = json.loads(await list_orders({"status": "completed"}))
orders.extend(page["orders"])
while page["next_cursor"]:
    page = json.loads(await list_orders({"status": "completed", "cursor": page["next_cursor"]}))
    orders.extend(page["orders"])
sums, counts = {}, {}
for order in orders:
    tier = tiers[order["customer_id"]]
    sums[tier] = sums.get(tier, 0) + order["total"]
    counts[tier] = counts.get(tier, 0) + 1
await submit_answer({"averages": {tier: round(sums[tier] / counts[tier], 2) for tier in ("bronze", "silver", "gold") if counts.get(tier)}})
