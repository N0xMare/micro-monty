import json

products = json.loads(await list_products({}))
top = sorted(products, key=lambda product: (-(product["price"] * product["stock"]), product["id"]))[:5]
body = "Top 5 products by stock value:\n" + "\n".join(
    f"{rank}. {product['name']} ({product['id']}) - ${product['price'] * product['stock']:,.2f}"
    for rank, product in enumerate(top, 1)
)
await send_email({"to": "inventory@example.com", "subject": "Stock value report", "body": body})
