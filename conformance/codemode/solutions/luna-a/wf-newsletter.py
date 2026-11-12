import json

sent = set()
page_num = 1
while True:
    page = json.loads(await list_customers({"region": "West", "tier": "gold", "page": page_num}))
    for customer in page["customers"]:
        address = customer["email"]
        if address in sent:
            continue
        sent.add(address)
        first_name = customer["name"].split()[0]
        await send_email({"to": address, "subject": "March newsletter", "body": f"Hello {first_name},\n\nWe’re excited to share our March updates and new offerings.\n"})
    if page_num >= page["total_pages"]:
        break
    page_num += 1
