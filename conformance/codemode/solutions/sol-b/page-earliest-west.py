import asyncio
import json

first = json.loads(await list_customers({"region": "West", "page": 1}))
rest = await asyncio.gather(*(list_customers({"region": "West", "page": page}) for page in range(2, first["total_pages"] + 1)))
customers = first["customers"] + [customer for raw in rest for customer in json.loads(raw)["customers"]]
earliest = min(customers, key=lambda customer: (customer["signup_date"], customer["id"]))
await submit_answer({"name": earliest["name"], "signup_date": earliest["signup_date"]})
