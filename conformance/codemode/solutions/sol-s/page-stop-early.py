args = {}
found = None
while found is None:
    result = await list_orders(args)
    found = next((order for order in result["orders"] if order["currency"] == "USD" and order["total"] > 3000), None)
    if found is not None or result["next_cursor"] is None:
        break
    args["cursor"] = result["next_cursor"]
if found is None:
    raise RuntimeError("No order above 3000 USD found")
await submit_answer({"order_id": found["id"]})
