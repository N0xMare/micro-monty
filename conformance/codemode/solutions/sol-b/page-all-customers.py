import asyncio
import json
from collections import Counter

first = json.loads(await list_customers({"page": 1}))
rest = await asyncio.gather(*(list_customers({"page": page}) for page in range(2, first["total_pages"] + 1)))
counts = Counter(customer["country"] for customer in first["customers"])
for raw in rest:
    counts.update(customer["country"] for customer in json.loads(raw)["customers"])
await submit_answer({"countries": dict(counts)})
