counts = {region: 0 for region in ("North", "South", "East", "West")}
page_number = 1
while True:
    page = await list_customers({"tier": "gold", "page": page_number})
    for customer in page["customers"]:
        counts[customer["region"]] = counts.get(customer["region"], 0) + 1
    if page_number >= page["total_pages"]:
        break
    page_number += 1
await submit_answer({"counts": counts})
