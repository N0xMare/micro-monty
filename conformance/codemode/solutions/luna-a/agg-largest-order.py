import json
orders, cursor = [], None
while True:
    args = {"status": "completed"}
    if cursor: args["cursor"] = cursor
    r = json.loads(await list_orders(args)); orders.extend(r["orders"])
    cursor = r["next_cursor"]
    if not cursor: break
o = json.loads(await get_order({"order_id": max(orders, key=lambda x: x["total"])["id"]}))
c = json.loads(await get_customer({"customer_id": o["customer_id"]}))
names = set()
for item in o["items"]: names.add(json.loads(await get_product({"product_id": item["product_id"]}))["name"])
await submit_answer({"order_id": o["id"], "customer": c["name"], "products": sorted(names)})
