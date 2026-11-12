import asyncio, json
from datetime import date
from collections import Counter
first = json.loads(await list_customers({"page": 1}))
rest = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))
customers = first["customers"] + [c for raw in rest for c in json.loads(raw)["customers"]]
quarters = Counter()
for c in customers:
    d = date.fromisoformat(c["signup_date"])
    quarters[f"{d.year}-Q{(d.month - 1) // 3 + 1}"] += 1
await submit_answer({"quarters": dict(sorted(quarters.items()))})
