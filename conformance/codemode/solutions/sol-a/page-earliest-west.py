import json

customers = []
number = 1
while True:
    page = json.loads(await list_customers({"region": "West", "page": number}))
    customers.extend(page["customers"])
    if page["page"] >= page["total_pages"]:
        break
    number = page["page"] + 1
earliest = min(customers, key=lambda customer: (customer["signup_date"], customer["id"]))
await submit_answer({"name": earliest["name"], "signup_date": earliest["signup_date"]})
