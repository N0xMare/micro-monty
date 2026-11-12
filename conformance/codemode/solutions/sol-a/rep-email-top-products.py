import json
from decimal import Decimal

products = json.loads(await list_products({}), parse_float=Decimal)
products.sort(key=lambda p: (-p["price"] * p["stock"], p["id"]))
lines = ["Top 5 products by stock value:"]
for i, p in enumerate(products[:5], 1):
    lines.append(f"{i}. {p['name']} ({p['id']}) - ${p['price'] * p['stock']:,.2f}")
await send_email({"to": "inventory@example.com", "subject": "Stock value report", "body": "\n".join(lines)})
