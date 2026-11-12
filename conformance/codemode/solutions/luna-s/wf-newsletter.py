import asyncio

customers = []
page = 1
while True:
    result = await list_customers({"region": "West", "tier": "gold", "page": page})
    customers.extend(result["customers"])
    if page >= result["total_pages"]: break
    page += 1
recipients = {}
for customer in customers:
    email = customer["email"]
    recipients.setdefault(email, customer["name"].split()[0])
await asyncio.gather(*(send_email({
    "to": email,
    "subject": "March newsletter",
    "body": f"Hello {first_name},\n\nWe’re excited to share our latest updates and offers this March!",
}) for email, first_name in recipients.items()))
