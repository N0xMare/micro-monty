import asyncio, json

orders = []
cursor = None
while True:
    args = {"status": "completed"}
    if cursor:
        args["cursor"] = cursor
    res = json.loads(await list_orders(args))
    orders.extend(res["orders"])
    cursor = res.get("next_cursor")
    if not cursor:
        break

completed = [o for o in orders if o["status"] == "completed"]
top = max(completed, key=lambda o: o["total"])
print("largest:", top, "ties:", [o["id"] for o in completed if o["total"] == top["total"]])

order_s, cust_s = await asyncio.gather(
    get_order({"order_id": top["id"]}),
    get_customer({"customer_id": top["customer_id"]}),
)
order = json.loads(order_s)
customer = json.loads(cust_s)
pids = sorted({it["product_id"] for it in order["items"]})
prods = [json.loads(p) for p in await asyncio.gather(*[get_product({"product_id": pid}) for pid in pids])]
names = sorted({p["name"] for p in prods})
print(order["id"], customer["name"], names)
print(await submit_answer({"order_id": order["id"], "customer": customer["name"], "products": names}))
