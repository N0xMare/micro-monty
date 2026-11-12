orders = []
cursor = None
while True:
    args = {"customer_id": 9}
    if cursor is not None:
        args["cursor"] = cursor
    page = await list_orders(args)
    orders.extend(page["orders"])
    cursor = page["next_cursor"]
    if cursor is None:
        break
await submit_answer({"order_ids": sorted(order["id"] for order in orders)})
