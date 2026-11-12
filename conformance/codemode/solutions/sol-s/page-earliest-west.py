customers = []
page = 1
while True:
    result = await list_customers({"region": "West", "page": page})
    customers.extend(result["customers"])
    if page >= result["total_pages"]:
        break
    page += 1
first = min(customers, key=lambda customer: (customer["signup_date"], customer["id"]))
await submit_answer({"name": first["name"], "signup_date": first["signup_date"]})
