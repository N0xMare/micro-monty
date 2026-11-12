args = {"customer_id": 9}
ids = []
while True:
    result = await list_orders(args)
    ids.extend(order["id"] for order in result["orders"])
    if result["next_cursor"] is None:
        break
    args["cursor"] = result["next_cursor"]
await submit_answer({"order_ids": sorted(ids)})
