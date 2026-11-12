from collections import Counter

first = await list_customers({"page": 1})
total_pages = first["total_pages"]
rest = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, total_pages + 1)))
counts = Counter()
for resp in [first, *rest]:
    for c in resp["customers"]:
        counts[c["country"]] += 1
print(dict(counts))
await submit_answer({"countries": dict(counts)})
