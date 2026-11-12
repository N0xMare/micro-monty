import json

customers, page_no = [], 1
while True:
    page = json.loads(await list_customers({"region": "West", "tier": "gold", "page": page_no}))
    customers.extend(page["customers"])
    if page_no >= page["total_pages"]: break
    page_no += 1
sent = set()
for customer in customers:
    address = customer["email"]
    if address in sent: continue
    sent.add(address)
    first = customer["name"].split()[0]
    await send_email({"to": address, "subject": "March newsletter", "body": f"Hello {first},\n\nWe’re pleased to share our March updates and new offerings. Thank you for being with us!"})
