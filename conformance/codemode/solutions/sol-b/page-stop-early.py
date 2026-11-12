import json

args = {}
while True:
    page = json.loads(await list_orders(args))
    match = next((order for order in page["orders"] if order["total"] > 3000 and order["currency"] == "USD"), None)
    if match is not None:
        await submit_answer({"order_id": match["id"]})
        break
    if page["next_cursor"] is None:
        raise ValueError("No order above 3000 USD found")
    args = {"cursor": page["next_cursor"]}
