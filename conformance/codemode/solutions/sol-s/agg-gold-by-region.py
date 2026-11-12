import asyncio

first = await list_customers({"tier": "gold", "page": 1})
pages = await asyncio.gather(*(list_customers({"tier": "gold", "page": p}) for p in range(2, first["total_pages"] + 1)))
counts = dict.fromkeys(("North", "South", "East", "West"), 0)
for page in [first, *pages]:
    for customer in page["customers"]:
        counts[customer["region"]] += 1
await submit_answer({"counts": counts})
