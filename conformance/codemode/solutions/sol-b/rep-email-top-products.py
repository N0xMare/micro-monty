import json

products = json.loads(await list_products({}))
top = sorted(products, key=lambda p: (-p["price"] * p["stock"], p["id"]))[:5]
lines = ["Top 5 products by stock value:"]
lines.extend(f"{i}. {p['name']} ({p['id']}) - ${p['price'] * p['stock']:,.2f}" for i, p in enumerate(top, 1))
await send_email({"to": "inventory@example.com", "subject": "Stock value report", "body": "\n".join(lines)})
