import asyncio

orders = []
cursor = None
while True:
    args = {"status": "completed"}
    if cursor:
        args["cursor"] = cursor
    res = await list_orders(args)
    orders.extend(res["orders"])
    cursor = res.get("next_cursor")
    if not cursor:
        break

completed = [o for o in orders if o["status"] == "completed"]
best = max(completed, key=lambda o: o["total"])
ties = [o["id"] for o in completed if o["total"] == best["total"]]
print("largest:", best, "ties:", ties)

order, customer = await asyncio.gather(
    get_order({"order_id": best["id"]}),
    get_customer({"customer_id": best["customer_id"]}),
)
pids = sorted({it["product_id"] for it in order["items"]})
products = await asyncio.gather(*(get_product({"product_id": p}) for p in pids))
names = sorted({p["name"] for p in products})

answer = {"order_id": best["id"], "customer": customer["name"], "products": names}
print(answer)
print(await submit_answer(answer))
