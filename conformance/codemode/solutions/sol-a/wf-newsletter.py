import json
seen = set()
page_number = 1
while True:
    page = json.loads(await list_customers({"region": "West", "tier": "gold", "page": page_number}))
    for customer in page["customers"]:
        address = customer["email"]
        if address in seen:
            continue
        first_name = customer["name"].split()[0]
        await send_email({"to": address, "subject": "March newsletter", "body": f"Hello {first_name},\n\nOur March collection is here! Explore the latest products and updates in our shop."})
        seen.add(address)
    if page_number >= page["total_pages"]:
        break
    page_number += 1
