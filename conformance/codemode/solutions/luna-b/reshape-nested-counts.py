import asyncio, json
from collections import defaultdict

first = json.loads(await list_customers({"page": 1}))
pages = [first]
if first["total_pages"] > 1:
    pages.extend(json.loads(raw) for raw in await asyncio.gather(
        *(list_customers({"page": page}) for page in range(2, first["total_pages"] + 1))))
counts = defaultdict(lambda: defaultdict(int))
for page in pages:
    for customer in page["customers"]:
        counts[customer["region"]][customer["tier"]] += 1
await submit_answer({"counts": {region: dict(tiers) for region, tiers in counts.items()}})
