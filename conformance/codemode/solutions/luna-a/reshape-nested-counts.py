import asyncio
import json
from collections import defaultdict

first = json.loads(await list_customers({"page": 1}))
pages = range(2, first["total_pages"] + 1)
results = await asyncio.gather(*(list_customers({"page": page}) for page in pages))
customers = first["customers"] + [customer for result in results for customer in json.loads(result)["customers"]]
counts = defaultdict(lambda: defaultdict(int))
for customer in customers:
    counts[customer["region"]][customer["tier"]] += 1
await submit_answer({"counts": {region: dict(tiers) for region, tiers in counts.items()}})
