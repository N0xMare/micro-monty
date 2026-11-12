import asyncio, json

first = json.loads(await list_customers({"region": "West", "page": 1}))
pages = [first]
total_pages = first.get("total_pages", 1)
if total_pages > 1:
    rest = await asyncio.gather(*(list_customers({"region": "West", "page": p}) for p in range(2, total_pages + 1)))
    pages += [json.loads(r) for r in rest]

west = [c for pg in pages for c in pg["customers"] if c["region"] == "West"]
earliest = min(west, key=lambda c: (c["signup_date"], c["id"]))
print(len(west), earliest)
print(await submit_answer({"name": earliest["name"], "signup_date": earliest["signup_date"]}))
