import json
from decimal import Decimal

args = {"status": "pending"}
count = 0
total = Decimal(0)
while True:
    page = json.loads(await list_orders(args), parse_float=Decimal)
    count += len(page["orders"])
    total += sum((Decimal(str(order["total"])) for order in page["orders"]), Decimal(0))
    if page["next_cursor"] is None:
        break
    args["cursor"] = page["next_cursor"]
await submit_answer({"count": count, "total": float(round(total, 2))})
