import asyncio
import json
from collections import Counter
from decimal import Decimal

async def orders():
    result = []
    args = {"customer_id": 17}
    while True:
        page = json.loads(await list_orders(args))
        result.extend(page["orders"])
        if page["next_cursor"] is None:
            return result
        args["cursor"] = page["next_cursor"]

customer_text, all_orders = await asyncio.gather(get_customer({"customer_id": 17}), orders())
total = sum((Decimal(str(o["total"])) for o in all_orders if o["status"] == "completed"), Decimal(0))
await submit_answer({"name": json.loads(customer_text)["name"], "counts": dict(Counter(o["status"] for o in all_orders)), "completed_total": float(round(total, 2))})
