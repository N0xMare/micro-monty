import asyncio
import json

orders = []
args = {"status": "completed"}
while True:
    page = json.loads(await list_orders(args))
    orders.extend(page["orders"])
    if page["next_cursor"] is None:
        break
    args["cursor"] = page["next_cursor"]
largest = max(orders, key=lambda o: (o["total"], -o["id"]))
order_text, customer_text = await asyncio.gather(get_order({"order_id": largest["id"]}), get_customer({"customer_id": largest["customer_id"]}))
product_ids = sorted({i["product_id"] for i in json.loads(order_text)["items"]})
products = await asyncio.gather(*(get_product({"product_id": p}) for p in product_ids))
await submit_answer({"order_id": largest["id"], "customer": json.loads(customer_text)["name"], "products": sorted({json.loads(p)["name"] for p in products})})
