import asyncio, json

orders, cursor = [], None
while True:
    args = {"status": "completed"}
    if cursor:
        args["cursor"] = cursor
    res = json.loads(await list_orders(args))
    orders += res["orders"]
    cursor = res.get("next_cursor")
    if not cursor:
        break

completed = [o for o in orders if o["status"] == "completed"]
best = max(completed, key=lambda o: o["total"])
ties = [o for o in completed if o["total"] == best["total"]]
print(len(completed), best, "ties:", len(ties))

order_raw, cust_raw = await asyncio.gather(
    get_order({"order_id": best["id"]}),
    get_customer({"customer_id": best["customer_id"]}),
)
order = json.loads(order_raw)
cust = json.loads(cust_raw)

pids = sorted({it["product_id"] for it in order["items"]})
prods = await asyncio.gather(*(get_product({"product_id": p}) for p in pids))
names = sorted({json.loads(p)["name"] for p in prods})

answer = {"order_id": best["id"], "customer": cust["name"], "products": names}
print(answer)
print(await submit_answer(answer))
