customers, page = [], 1
while True:
    res = await list_customers({"region": "West", "tier": "gold", "page": page})
    customers.extend(res["customers"])
    if page >= res["total_pages"]:
        break
    page += 1

sent, seen = [], set()
for c in customers:
    if c.get("region") != "West" or c.get("tier") != "gold":
        continue
    email = (c.get("email") or "").strip()
    key = email.lower()
    if not email or key in seen:
        continue
    seen.add(key)
    first = c["name"].split()[0]
    body = (
        f"Hello {first},\n\n"
        "Our March newsletter is here! This month we're rolling out new products, "
        "faster shipping for gold members, and a spring sale starting next week.\n\n"
        "Thanks for being a valued customer."
    )
    await send_email({"to": email, "subject": "March newsletter", "body": body})
    sent.append(email)
print(len(sent), sent)

if "submit_answer" in globals():
    await submit_answer({"sent_to": sent, "count": len(sent)})
