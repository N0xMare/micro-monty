import asyncio, json

first = json.loads(await list_customers({"page": 1}))
rest = [json.loads(s) for s in await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))]
counts = {}
for page in [first] + rest:
    for c in page["customers"]:
        tiers = counts.setdefault(c["region"], {})
        tiers[c["tier"]] = tiers.get(c["tier"], 0) + 1
await submit_answer({"counts": counts})
