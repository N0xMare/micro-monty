import asyncio
from datetime import date
from collections import Counter

first = await list_customers({"page": 1})
pages = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))
quarters = Counter()
for page in [first, *pages]:
    for customer in page["customers"]:
        day = date.fromisoformat(customer["signup_date"])
        quarters[f"{day.year}-Q{(day.month - 1) // 3 + 1}"] += 1
await submit_answer({"quarters": dict(sorted(quarters.items()))})
