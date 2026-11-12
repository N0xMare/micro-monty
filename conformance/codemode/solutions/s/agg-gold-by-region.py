import asyncio

regions = ["North", "South", "East", "West"]

async def gold_in(region):
    first = await list_customers({"region": region, "tier": "gold", "page": 1})
    customers = list(first["customers"])
    pages = first.get("total_pages", 1) or 1
    if pages > 1:
        rest = await asyncio.gather(*(list_customers({"region": region, "tier": "gold", "page": p}) for p in range(2, pages + 1)))
        for r in rest:
            customers.extend(r["customers"])
    ids = {c["id"] for c in customers if c.get("tier") == "gold" and c.get("region") == region}
    return len(ids)

results = await asyncio.gather(*(gold_in(r) for r in regions))
counts = dict(zip(regions, results))
print(counts)
print(await submit_answer({"counts": counts}))
