import asyncio, json

# Customers: fetch page 1, then remaining pages concurrently
first = json.loads(await list_customers({}))
pages = [first]
if first["total_pages"] > 1:
    rest = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))
    pages += [json.loads(r) for r in rest]
tier_of = {c["id"]: c["tier"] for pg in pages for c in pg["customers"]}

# Completed orders: follow cursor
orders = []
cursor = None
while True:
    args = {"status": "completed"}
    if cursor:
        args["cursor"] = cursor
    res = json.loads(await list_orders(args))
    orders += res["orders"]
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
print(len(orders), counts, averages)
print(await submit_answer({"averages": averages}))
