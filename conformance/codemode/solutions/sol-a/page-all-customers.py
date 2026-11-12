import json
from collections import Counter

counts = Counter()
number = 1
while True:
    page = json.loads(await list_customers({"page": number}))
    counts.update(customer["country"] for customer in page["customers"])
    if page["page"] >= page["total_pages"]:
        break
    number = page["page"] + 1
await submit_answer({"countries": dict(counts)})
