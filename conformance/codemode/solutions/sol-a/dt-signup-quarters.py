import asyncio
import json
from collections import Counter
from datetime import date

first = json.loads(await list_customers({"page": 1}))
pages = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))
customers = first["customers"] + [c for page in pages for c in json.loads(page)["customers"]]
quarters = Counter()
for customer in customers:
    day = date.fromisoformat(customer["signup_date"])
    quarters[f"{day.year}-Q{(day.month - 1) // 3 + 1}"] += 1
await submit_answer({"quarters": dict(sorted(quarters.items()))})
