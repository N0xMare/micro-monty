cursor = None
found = None
while True:
    args = {}
    if cursor is not None:
        args["cursor"] = cursor
    page = await list_orders(args)
    match = next((o for o in page["orders"] if o["currency"] == "USD" and o["total"] > 3000), None)
    if match is not None:
        found = match["id"]
        break
    cursor = page["next_cursor"]
    if cursor is None:
        break
await submit_answer({"order_id": found})
