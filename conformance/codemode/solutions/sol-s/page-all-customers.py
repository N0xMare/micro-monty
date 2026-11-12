from collections import Counter

counts = Counter()
page = 1
while True:
    result = await list_customers({"page": page})
    counts.update(customer["country"] for customer in result["customers"])
    if page >= result["total_pages"]:
        break
    page += 1
await submit_answer({"countries": dict(counts)})
