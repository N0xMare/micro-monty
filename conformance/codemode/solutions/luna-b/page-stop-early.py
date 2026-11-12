import json

cursor = None
found = None
while True:
    arguments = {}
    if cursor is not None:
        arguments["cursor"] = cursor
    page = json.loads(await list_orders(arguments))
    match = next((order for order in page["orders"] if order["currency"] == "USD" and order["total"] > 3000), None)
    if match is not None:
        found = match["id"]
        break
    cursor = page["next_cursor"]
    if cursor is None:
        break
await submit_answer({"order_id": found})
