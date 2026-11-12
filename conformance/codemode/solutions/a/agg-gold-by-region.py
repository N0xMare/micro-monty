import asyncio, json

async def all_gold(region):
    first = json.loads(await list_customers({"region": region, "tier": "gold", "page": 1}))
    custs = list(first["customers"])
    if first["total_pages"] > 1:
        rest = await asyncio.gather(*[
            list_customers({"region": region, "tier": "gold", "page": p})
            for p in range(2, first["total_pages"] + 1)
        ])
        for r in rest:
            custs.extend(json.loads(r)["customers"])
    return [c for c in custs if c["tier"] == "gold" and c["region"] == region]

regions = ["North", "South", "East", "West"]
results = await asyncio.gather(*[all_gold(r) for r in regions])
counts = {r: len({c["id"] for c in cs}) for r, cs in zip(regions, results)}
print(counts)
print(await submit_answer({"counts": counts}))
