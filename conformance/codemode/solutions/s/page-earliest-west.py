first = await list_customers({"region": "West", "page": 1})
rest = await asyncio.gather(*(list_customers({"region": "West", "page": p}) for p in range(2, first["total_pages"] + 1)))
customers = [c for resp in [first, *rest] for c in resp["customers"] if c["region"] == "West"]
earliest = min(customers, key=lambda c: (c["signup_date"], c["id"]))
print(len(customers), earliest)
await submit_answer({"name": earliest["name"], "signup_date": earliest["signup_date"]})
