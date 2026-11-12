customers = []
page_num = 1
while True:
    page = await list_customers({"region": "West", "page": page_num})
    customers.extend(page["customers"])
    if page_num >= page["total_pages"]:
        break
    page_num += 1
first = min(customers, key=lambda c: (c["signup_date"], c["id"]))
await submit_answer({"name": first["name"], "signup_date": first["signup_date"]})
