import asyncio, json

# Fetch all customers (paginated) to map id -> tier
first = json.loads(await list_customers({"page": 1}))
pages = [first]
if first["total_pages"] > 1:
    rest = await asyncio.gather(*[list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)])
    pages += [json.loads(r) for r in rest]
tier_of = {c["id"]: c["tier"] for pg in pages for c in pg["customers"]}

# Fetch all completed orders (cursor pagination)
orders = []
cursor = None
while True:
    args = {"status": "completed"}
    if cursor:
        args["cursor"] = cursor
    res = json.loads(await list_orders(args))
    orders.extend(res["orders"])
    cursor = res.get("next_cursor")
    if not cursor:
        break

sums, counts = {}, {}
for o in orders:
    if o["status"] != "completed":
        continue
    t = tier_of.get(o["customer_id"])
    if t is None:
        continue
    sums[t] = sums.get(t, 0.0) + o["total"]
    counts[t] = counts.get(t, 0) + 1

averages = {t: round(sums[t] / counts[t], 2) for t in ("bronze", "silver", "gold") if counts.get(t)}
print(averages, {t: counts.get(t, 0) for t in ("bronze", "silver", "gold")})
print(await submit_answer({"averages": averages}))
