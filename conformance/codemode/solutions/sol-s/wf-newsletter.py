page = 1
sent = set()
while True:
    result = await list_customers({"region": "West", "tier": "gold", "page": page})
    for customer in result["customers"]:
        address = customer["email"]
        key = address.strip().lower()
        if key not in sent:
            first_name = customer["name"].split()[0]
            await send_email({"to": address, "subject": "March newsletter", "body": f"Hello {first_name},\nWe are excited to share our March product updates. Explore the latest additions to our shop this month!"})
            sent.add(key)
    if page >= result["total_pages"]:
        break
    page += 1
