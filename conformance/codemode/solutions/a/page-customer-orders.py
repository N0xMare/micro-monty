import json

ids = set()
cursor = None
while True:
    args = {"customer_id": 9}
    if cursor:
        args["cursor"] = cursor
    res = json.loads(await list_orders(args))
    for o in res["orders"]:
        if o["customer_id"] == 9:
            ids.add(o["id"])
    cursor = res.get("next_cursor")
    if not cursor:
        break
order_ids = sorted(ids)
print(len(order_ids), order_ids)
await submit_answer({"order_ids": order_ids})
