import asyncio
from collections import Counter, defaultdict

first = await list_customers({"page": 1})
rest = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))
counts = defaultdict(Counter)
for page in [first, *rest]:
    for customer in page["customers"]:
        counts[customer["region"]][customer["tier"]] += 1
await submit_answer({"counts": {r: dict(tiers) for r, tiers in counts.items()}})
