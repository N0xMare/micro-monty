from collections import Counter

counts = Counter()
page_number = 1
while True:
    page = await list_customers({"page": page_number})
    for customer in page["customers"]:
        year, month = map(int, customer["signup_date"].split("-")[:2])
        counts[f"{year}-Q{(month - 1) // 3 + 1}"] += 1
    if page_number >= page["total_pages"]:
        break
    page_number += 1
await submit_answer({"quarters": dict(sorted(counts.items()))})
