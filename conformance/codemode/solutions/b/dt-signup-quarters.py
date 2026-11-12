import json, asyncio
from collections import Counter

first = json.loads(await list_customers({"page": 1}))
pages = [first]
if first["total_pages"] > 1:
    rest = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))
    pages += [json.loads(r) for r in rest]

customers = {c["id"]: c for p in pages for c in p["customers"]}
counts = Counter()
for c in customers.values():
    y, m = int(c["signup_date"][:4]), int(c["signup_date"][5:7])
    counts[f"{y}-Q{(m - 1) // 3 + 1}"] += 1

quarters = dict(sorted(counts.items()))
print(len(customers), quarters)
print(await submit_answer({"quarters": quarters}))
