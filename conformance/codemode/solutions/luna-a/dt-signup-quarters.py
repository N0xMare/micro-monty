import json
quarters, page = {}, 1
while True:
    r = json.loads(await list_customers({"page": page}))
    for c in r["customers"]:
        y, m = int(c["signup_date"][:4]), int(c["signup_date"][5:7]); label = f"{y}-Q{(m-1)//3+1}"
        quarters[label] = quarters.get(label, 0) + 1
    if page >= r["total_pages"]: break
    page += 1
await submit_answer({"quarters": quarters})
