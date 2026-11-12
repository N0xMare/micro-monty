import asyncio
import json
from collections import Counter

first = json.loads(await list_customers({"page": 1}))
pages = await asyncio.gather(*(list_customers({"page": page}) for page in range(2, first["total_pages"] + 1)))
customers = first["customers"][:]
for response in pages:
    customers.extend(json.loads(response)["customers"])
countries = dict(Counter(customer["country"] for customer in customers))
await submit_answer({"countries": countries})
