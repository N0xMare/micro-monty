import asyncio
import json

orders = await asyncio.gather(*(get_order({"order_id": order_id}) for order_id in range(1001, 1041)))
total = sum(json.loads(order)["total"] for order in orders)
refunded = sum(json.loads(order)["status"] == "refunded" for order in orders)
await submit_answer({"total": round(total, 2), "refunded": refunded})
