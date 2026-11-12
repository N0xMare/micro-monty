import asyncio
from decimal import Decimal

orders = await asyncio.gather(*(get_order({"order_id": order_id}) for order_id in range(1001, 1041)))
total = sum((Decimal(str(order["total"])) for order in orders), Decimal(0))
await submit_answer({"total": float(round(total, 2)), "refunded": sum(order["status"] == "refunded" for order in orders)})
