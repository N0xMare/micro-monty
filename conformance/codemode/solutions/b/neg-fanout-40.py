import asyncio, json

ids = list(range(1001, 1041))
results = await asyncio.gather(*(get_order({"order_id": i}) for i in ids), return_exceptions=True)
orders, failed = {}, []
for i, r in zip(ids, results):
    if isinstance(r, Exception):
        failed.append(i)
    else:
        orders[i] = json.loads(r)
for i in failed:  # retry any failures one by one
    try:
        orders[i] = json.loads(await get_order({"order_id": i}))
    except Exception as e:
        print("order", i, "failed:", e)
total = round(sum(o["total"] for o in orders.values()), 2)
refunded = sum(1 for o in orders.values() if o["status"] == "refunded")
print(len(orders), "orders; total", total, "refunded", refunded, "currencies", {o["currency"] for o in orders.values()})
print(await submit_answer({"total": total, "refunded": refunded}))
