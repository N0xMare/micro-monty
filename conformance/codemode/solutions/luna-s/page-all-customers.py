from collections import Counter

first = await list_customers({"page": 1})
customers = list(first["customers"])
for page_num in range(2, first["total_pages"] + 1):
    page = await list_customers({"page": page_num})
    customers.extend(page["customers"])
counts = dict(Counter(c["country"] for c in customers))
await submit_answer({"countries": counts})
