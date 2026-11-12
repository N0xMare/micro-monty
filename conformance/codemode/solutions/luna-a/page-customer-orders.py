import json

cursor = None
ids = []
while True:
    args = {"customer_id": 9}
    if cursor is not None:
        args["cursor"] = cursor
    page = json.loads(await list_orders(args))
    ids.extend(order["id"] for order in page["orders"])
    cursor = page["next_cursor"]
    if cursor is None:
        break
await submit_answer({"order_ids": sorted(ids)})
