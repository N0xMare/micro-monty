import asyncio, json

first = json.loads(await list_customers({"tier": "gold", "page": 1}))
pages = [first] + await asyncio.gather(*(list_customers({"tier": "gold", "page": p}) for p in range(2, first["total_pages"] + 1)))
counts = {r: 0 for r in ("North", "South", "East", "West")}
for page in pages:
    for customer in page["customers"]:
        counts[customer["region"]] += 1
await submit_answer({"counts": counts})
