import asyncio
import json
first = json.loads(await list_customers({"region": "West", "tier": "gold", "page": 1}))
pages = await asyncio.gather(*(list_customers({"region": "West", "tier": "gold", "page": page}) for page in range(2, first["total_pages"] + 1)))
customers = first["customers"] + [customer for raw in pages for customer in json.loads(raw)["customers"]]
recipients = {}
for customer in customers:
    recipients.setdefault(customer["email"], customer)
await asyncio.gather(*(send_email({"to": address, "subject": "March newsletter", "body": f"Hello {customer['name'].split()[0]},\nOur March product updates are now available. Thank you for being a valued customer!"}) for address, customer in recipients.items()))
