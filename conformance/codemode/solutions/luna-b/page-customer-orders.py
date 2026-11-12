import json

order_ids = []
cursor = None
while True:
    arguments = {"customer_id": 9}
    if cursor is not None:
        arguments["cursor"] = cursor
    page = json.loads(await list_orders(arguments))
    order_ids.extend(order["id"] for order in page["orders"])
    cursor = page["next_cursor"]
    if cursor is None:
        break
await submit_answer({"order_ids": sorted(order_ids)})
