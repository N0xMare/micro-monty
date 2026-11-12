import asyncio
import json

first = json.loads(await list_customers({"region": "West", "page": 1}))
pages = await asyncio.gather(*(list_customers({"region": "West", "page": page}) for page in range(2, first["total_pages"] + 1)))
customers = first["customers"][:]
for response in pages:
    customers.extend(json.loads(response)["customers"])
earliest = min(customers, key=lambda customer: customer["signup_date"])
await submit_answer({"name": earliest["name"], "signup_date": earliest["signup_date"]})
