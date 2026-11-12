import json

customers = []
page_number = 1
while True:
    page = json.loads(await list_customers({"region": "West", "page": page_number}))
    customers.extend(page["customers"])
    if page_number >= page["total_pages"]:
        break
    page_number += 1
first = min(customers, key=lambda customer: (customer["signup_date"], customer["id"]))
await submit_answer({"name": first["name"], "signup_date": first["signup_date"]})
