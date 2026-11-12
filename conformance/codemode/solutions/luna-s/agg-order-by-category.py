order = await get_order({"order_id": 1042})
product_ids = sorted({item["product_id"] for item in order["items"]})
products = await asyncio.gather(*(get_product({"product_id": product_id}) for product_id in product_ids))
category_by_id = {product["id"]: product["category"] for product in products}
subtotals = {}
for item in order["items"]:
    category = category_by_id[item["product_id"]]
    subtotals[category] = subtotals.get(category, 0) + int(item["qty"]) * float(item["unit_price"])
await submit_answer({"subtotals": {category: round(total, 2) for category, total in subtotals.items()}})
