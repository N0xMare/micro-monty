import json
from collections import Counter

counts = Counter()
first = json.loads(await list_customers({"page": 1}))
pages = [first]
total_pages = first["total_pages"]
if total_pages > 1:
    import asyncio
    rest = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, total_pages + 1)))
    pages += [json.loads(r) for r in rest]
seen = set()
for pg in pages:
    for c in pg["customers"]:
        if c["id"] in seen:
            continue
        seen.add(c["id"])
        counts[c["country"]] += 1
print(len(seen), dict(counts))
await submit_answer({"countries": dict(counts)})
