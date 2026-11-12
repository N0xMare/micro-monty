import asyncio, json

async def gold_in(region):
    first = json.loads(await list_customers({"region": region, "tier": "gold"}))
    pages = [first]
    if first["total_pages"] > 1:
        rest = await asyncio.gather(*(list_customers({"region": region, "tier": "gold", "page": p})
                                      for p in range(2, first["total_pages"] + 1)))
        pages += [json.loads(r) for r in rest]
    ids = {c["id"] for pg in pages for c in pg["customers"]
           if c["tier"] == "gold" and c["region"] == region}
    return len(ids)

regions = ["North", "South", "East", "West"]
values = await asyncio.gather(*(gold_in(r) for r in regions))
counts = dict(zip(regions, values))
print(counts)
print(await submit_answer({"counts": counts}))
