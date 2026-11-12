import asyncio
from collections import Counter
from decimal import Decimal

customer, page = await asyncio.gather(get_customer({"customer_id": 17}), list_orders({"customer_id": 17}))
orders = list(page["orders"])
while page["next_cursor"] is not None:
    page = await list_orders({"customer_id": 17, "cursor": page["next_cursor"]})
    orders.extend(page["orders"])
total = sum((Decimal(str(o["total"])) for o in orders if o["status"] == "completed"), Decimal(0))
await submit_answer({"name": customer["name"], "counts": dict(Counter(o["status"] for o in orders)), "completed_total": float(round(total, 2))})
