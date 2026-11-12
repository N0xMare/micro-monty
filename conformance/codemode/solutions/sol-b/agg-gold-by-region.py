import asyncio, json
first = json.loads(await list_customers({"tier": "gold", "page": 1}))
rest = await asyncio.gather(*(list_customers({"tier": "gold", "page": p}) for p in range(2, first["total_pages"] + 1)))
counts = dict.fromkeys(["North", "South", "East", "West"], 0)
for c in first["customers"] + [c for raw in rest for c in json.loads(raw)["customers"]]:
    counts[c["region"]] += 1
await submit_answer({"counts": counts})
