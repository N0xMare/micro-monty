import json

cursor = None
found_id = None
while True:
    args = {}
    if cursor is not None:
        args["cursor"] = cursor
    page = json.loads(await list_orders(args))
    match = next((order for order in page["orders"] if order["total"] > 3000), None)
    if match is not None:
        found_id = match["id"]
        break
    cursor = page["next_cursor"]
    if cursor is None:
        break
await submit_answer({"order_id": found_id})
