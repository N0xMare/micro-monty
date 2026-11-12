import json

customers, page = [], 1
while True:
    res = json.loads(await list_customers({"region": "West", "tier": "gold", "page": page}))
    customers.extend(c for c in res["customers"] if c["region"] == "West" and c["tier"] == "gold")
    if page >= res["total_pages"]:
        break
    page += 1

sent, seen = [], set()
for c in customers:
    email = c["email"].strip()
    key = email.lower()
    if not email or key in seen:
        continue
    seen.add(key)
    first = c["name"].split()[0] if c["name"].strip() else "there"
    body = (
        f"Hello {first},\n\n"
        "Our March newsletter is here! This month we're rolling out new spring arrivals, "
        "faster shipping for gold members, and an exclusive early-access sale next week.\n\n"
        "Thanks for being a valued customer."
    )
    await send_email({"to": email, "subject": "March newsletter", "body": body})
    sent.append(email)

print(json.dumps({"sent": sent, "count": len(sent)}))
if "submit_answer" in globals():
    await submit_answer({"sent": sent, "count": len(sent)})
