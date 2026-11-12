import asyncio
import json

first = json.loads(await list_customers({"tier": "gold", "page": 1}))
pages = await asyncio.gather(*(list_customers({"tier": "gold", "page": p}) for p in range(2, first["total_pages"] + 1)))
customers = first["customers"] + [c for page in pages for c in json.loads(page)["customers"]]
counts = dict.fromkeys(("North", "South", "East", "West"), 0)
for customer in customers:
    counts[customer["region"]] += 1
await submit_answer({"counts": counts})
