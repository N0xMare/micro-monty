import asyncio, json
orders, args = [], {"status": "completed"}
while True:
    page = json.loads(await list_orders(args))
    orders.extend(page["orders"])
    if not page["next_cursor"]:
        break
    args["cursor"] = page["next_cursor"]
largest = max(orders, key=lambda o: o["total"])
raw_order, raw_customer = await asyncio.gather(get_order({"order_id": largest["id"]}), get_customer({"customer_id": largest["customer_id"]}))
ids = sorted({i["product_id"] for i in json.loads(raw_order)["items"]})
products = await asyncio.gather(*(get_product({"product_id": pid}) for pid in ids))
await submit_answer({"order_id": largest["id"], "customer": json.loads(raw_customer)["name"], "products": sorted({json.loads(p)["name"] for p in products})})
