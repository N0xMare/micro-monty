import asyncio
first = await list_customers({"page": 1})
pages = [first]
if first["total_pages"] > 1:
    pages += await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))
counts = {}
for page in pages:
    for customer in page["customers"]:
        tiers = counts.setdefault(customer["region"], {})
        tier = customer["tier"]
        tiers[tier] = tiers.get(tier, 0) + 1
await submit_answer({"counts": counts})
