import asyncio
import json
from collections import defaultdict

first = json.loads(await list_customers({"page": 1}))
pages = [first]
total_pages = first.get("total_pages", 1)
if total_pages > 1:
    rest = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, total_pages + 1)))
    pages += [json.loads(r) for r in rest]

seen = set()
counts = defaultdict(lambda: defaultdict(int))
for page in pages:
    for c in page["customers"]:
        if c["id"] in seen:
            continue
        seen.add(c["id"])
        counts[c["region"]][c["tier"]] += 1

result = {region: dict(tiers) for region, tiers in sorted(counts.items())}
print(len(seen), "customers;", result)
print(await submit_answer({"counts": result}))
