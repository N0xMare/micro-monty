import asyncio, json
from collections import Counter
async def all_orders():
    orders, args = [], {"customer_id": 17}
    while True:
        page = json.loads(await list_orders(args))
        orders.extend(page["orders"])
        if not page["next_cursor"]:
            return orders
        args["cursor"] = page["next_cursor"]
raw, orders = await asyncio.gather(get_customer({"customer_id": 17}), all_orders())
await submit_answer({"name": json.loads(raw)["name"], "counts": dict(Counter(o["status"] for o in orders)), "completed_total": round(sum(o["total"] for o in orders if o["status"] == "completed"), 2)})
