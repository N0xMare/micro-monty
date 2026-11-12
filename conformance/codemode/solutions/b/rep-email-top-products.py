import json

products = json.loads(await list_products({}))
valued = sorted(((round(p["price"] * p["stock"], 2), p) for p in products),
                key=lambda vp: (-vp[0], vp[1]["id"]))[:5]

lines = ["Top 5 products by stock value:"]
for i, (v, p) in enumerate(valued, 1):
    lines.append(f"{i}. {p['name']} ({p['id']}) - ${v:,.2f}")
body = "\n".join(lines)

print(body)
print(await send_email({"to": "inventory@example.com", "subject": "Stock value report", "body": body}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"body": body}))
