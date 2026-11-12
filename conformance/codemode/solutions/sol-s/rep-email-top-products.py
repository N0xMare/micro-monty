from decimal import Decimal

products = await list_products({})
def value(p):
    return Decimal(str(p["price"])) * p["stock"]
top = sorted(products, key=lambda p: (-value(p), p["id"]))[:5]
lines = ["Top 5 products by stock value:"]
lines.extend(f"{i}. {p['name']} ({p['id']}) - ${value(p):,.2f}" for i, p in enumerate(top, 1))
await send_email({"to": "inventory@example.com", "subject": "Stock value report", "body": "\n".join(lines)})
