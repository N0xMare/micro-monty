import asyncio, json

first = json.loads(await list_customers({"page": 1}))
pages = [first] + await asyncio.gather(*(list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)))
quarters = {}
for page in pages:
    for c in page["customers"]:
        year, month = c["signup_date"].split("-")[:2]
        label = f"{year}-Q{(int(month) - 1) // 3 + 1}"
        quarters[label] = quarters.get(label, 0) + 1
await submit_answer({"quarters": quarters})
