import json

args = {}
found = None
while True:
    page = json.loads(await list_orders(args))
    found = next((order for order in page["orders"] if order["currency"] == "USD" and order["total"] > 3000), None)
    if found is not None or page["next_cursor"] is None:
        break
    args["cursor"] = page["next_cursor"]
if found is None:
    raise ValueError("No order above 3000 USD")
await submit_answer({"order_id": found["id"]})
