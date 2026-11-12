page = await list_orders({"status": "completed"})
orders = list(page["orders"])
cursor = page["next_cursor"]
while cursor:
    page = await list_orders({"status": "completed", "cursor": cursor})
    orders.extend(page["orders"])
    cursor = page["next_cursor"]
largest = max(orders, key=lambda order: (float(order["total"]), -int(order["id"])))
order = await get_order({"order_id": largest["id"]})
customer = await get_customer({"customer_id": order["customer_id"]})
product_ids = sorted({item["product_id"] for item in order["items"]})
products = await asyncio.gather(*(get_product({"product_id": product_id}) for product_id in product_ids))
await submit_answer({"order_id": order["id"], "customer": customer["name"], "products": sorted({product["name"] for product in products})})
