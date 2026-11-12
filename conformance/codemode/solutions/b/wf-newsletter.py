import json

customers, page = [], 1
while True:
    res = json.loads(await list_customers({"region": "West", "tier": "gold", "page": page}))
    customers.extend(res["customers"])
    if page >= res["total_pages"]:
        break
    page += 1

seen, sent = set(), []
for c in customers:
    if c["region"] != "West" or c["tier"] != "gold":
        continue
    addr = c["email"].strip()
    if addr.lower() in seen:
        continue
    seen.add(addr.lower())
    first = c["name"].split()[0]
    await send_email({
        "to": addr,
        "subject": "March newsletter",
        "body": f"Hello {first},\n\nOur spring collection is now live, and as a gold member you get early access plus free shipping all through March.\n\nThanks for being with us!",
    })
    sent.append(addr)

print({"sent": len(sent), "recipients": sent})
# submit_answer is not in this cell's tool catalog, so the result is reported via print only.
