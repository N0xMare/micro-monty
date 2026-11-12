import asyncio, json

async def all_orders(args):
    out, cursor = [], None
    while True:
        a = dict(args)
        if cursor:
            a["cursor"] = cursor
        res = json.loads(await list_orders(a))
        out += res["orders"]
        cursor = res.get("next_cursor")
        if not cursor:
            return out

cust_raw, orders = await asyncio.gather(get_customer({"customer_id": 17}), all_orders({"customer_id": 17}))
cust = json.loads(cust_raw)
orders = [o for o in orders if o["customer_id"] == 17]

counts = {}
completed_total = 0.0
for o in orders:
    counts[o["status"]] = counts.get(o["status"], 0) + 1
    if o["status"] == "completed":
        completed_total += o["total"]

answer = {"name": cust["name"], "counts": counts, "completed_total": round(completed_total, 2)}
print(answer)
print(await submit_answer(answer))
