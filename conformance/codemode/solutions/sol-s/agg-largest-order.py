import asyncio

orders = []
args = {"status": "completed"}
while True:
    page = await list_orders(args)
    orders.extend(page["orders"])
    if page["next_cursor"] is None:
        break
    args = {"status": "completed", "cursor": page["next_cursor"]}
largest = max(orders, key=lambda o: (o["total"], -o["id"]))
order, customer = await asyncio.gather(get_order({"order_id": largest["id"]}), get_customer({"customer_id": largest["customer_id"]}))
products = await asyncio.gather(*(get_product({"product_id": pid}) for pid in sorted({i["product_id"] for i in order["items"]})))
await submit_answer({"order_id": order["id"], "customer": customer["name"], "products": sorted({p["name"] for p in products})})
