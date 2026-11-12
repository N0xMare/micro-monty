products = await list_products({})
top = sorted(products, key=lambda p: (-(p["price"] * p["stock"]), p["id"]))[:5]
lines = ["Top 5 products by stock value:"]
for i, p in enumerate(top, 1):
    lines.append(f"{i}. {p['name']} ({p['id']}) - ${p['price'] * p['stock']:,.2f}")
await send_email({"to": "inventory@example.com", "subject": "Stock value report", "body": "\n".join(lines)})
