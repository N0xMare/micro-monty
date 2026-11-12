import json
import asyncio

first = json.loads(await list_customers({"region": "West", "page": 1}))
pages = [first]
if first["total_pages"] > 1:
    rest = await asyncio.gather(*(list_customers({"region": "West", "page": p}) for p in range(2, first["total_pages"] + 1)))
    pages += [json.loads(r) for r in rest]
customers = [c for pg in pages for c in pg["customers"] if c["region"] == "West"]
earliest = min(customers, key=lambda c: (c["signup_date"], c["id"]))
print(len(customers), earliest)
await submit_answer({"name": earliest["name"], "signup_date": earliest["signup_date"]})
