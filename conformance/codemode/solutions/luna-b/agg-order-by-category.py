import asyncio, json

order = json.loads(await get_order({"order_id": 1042}))
products = await asyncio.gather(*(get_product({"product_id": item["product_id"]}) for item in order["items"]))
categories = {item["product_id"]: json.loads(product)["category"] for item, product in zip(order["items"], products)}
subtotals = {}
for item in order["items"]:
    category = categories[item["product_id"]]
    subtotals[category] = subtotals.get(category, 0) + item["qty"] * item["unit_price"]
await submit_answer({"subtotals": {k: round(v, 2) for k, v in subtotals.items()}})
