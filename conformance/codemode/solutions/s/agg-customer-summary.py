import asyncio

async def all_orders(customer_id):
    orders = []
    cursor = None
    while True:
        args = {"customer_id": customer_id}
        if cursor:
            args["cursor"] = cursor
        res = await list_orders(args)
        orders.extend(res["orders"])
        cursor = res.get("next_cursor")
        if not cursor:
            break
    return orders

customer, orders = await asyncio.gather(get_customer({"customer_id": 17}), all_orders(17))
orders = list({o["id"]: o for o in orders if o["customer_id"] == 17}.values())

counts = {}
completed_total = 0.0
for o in orders:
    counts[o["status"]] = counts.get(o["status"], 0) + 1
    if o["status"] == "completed":
        completed_total += o["total"]

answer = {"name": customer["name"], "counts": counts, "completed_total": round(completed_total, 2)}
print(answer)
print(await submit_answer(answer))
