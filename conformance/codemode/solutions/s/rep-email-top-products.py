products = await list_products({})

ranked = sorted(products, key=lambda p: (-(p["price"] * p["stock"]), p["id"]))[:5]
lines = ["Top 5 products by stock value:"]
for i, p in enumerate(ranked, 1):
    value = p["price"] * p["stock"]
    lines.append(f"{i}. {p['name']} ({p['id']}) - ${value:,.2f}")
body = "\n".join(lines)
print(body)

res = await send_email({"to": "inventory@example.com", "subject": "Stock value report", "body": body})
print(res)
