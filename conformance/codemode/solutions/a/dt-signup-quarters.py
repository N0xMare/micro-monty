import json
import asyncio
from collections import Counter

first = json.loads(await list_customers({"page": 1}))
pages = [first]
if first["total_pages"] > 1:
    rest = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))
    pages += [json.loads(r) for r in rest]
customers = {}
for p in pages:
    for c in p["customers"]:
        customers[c["id"]] = c
q = Counter()
for c in customers.values():
    d = c.get("signup_date")
    if not d:
        continue
    y, m = int(d[:4]), int(d[5:7])
    q[f"{y}-Q{(m - 1) // 3 + 1}"] += 1
quarters = dict(sorted(q.items()))
print(len(customers), quarters)
print(await submit_answer({"quarters": quarters}))
