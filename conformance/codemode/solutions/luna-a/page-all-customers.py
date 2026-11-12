import json
from collections import Counter

countries = Counter()
page_number = 1
while True:
    page = json.loads(await list_customers({"page": page_number}))
    countries.update(customer["country"] for customer in page["customers"])
    if page_number >= page["total_pages"]:
        break
    page_number += 1
await submit_answer({"countries": dict(countries)})
