import json

args = {"customer_id": 9}
ids = []
while True:
    page = json.loads(await list_orders(args))
    ids.extend(order["id"] for order in page["orders"])
    if page["next_cursor"] is None:
        break
    args = {"customer_id": 9, "cursor": page["next_cursor"]}
await submit_answer({"order_ids": sorted(ids)})
