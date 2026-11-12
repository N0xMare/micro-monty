from decimal import Decimal

args = {"status": "pending"}
count = 0
total = Decimal(0)
while True:
    result = await list_orders(args)
    count += len(result["orders"])
    total += sum((Decimal(str(order["total"])) for order in result["orders"]), Decimal(0))
    if result["next_cursor"] is None:
        break
    args["cursor"] = result["next_cursor"]
await submit_answer({"count": count, "total": float(round(total, 2))})
