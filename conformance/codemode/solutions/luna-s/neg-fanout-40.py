import asyncio

orders = await asyncio.gather(*(get_order({"order_id": order_id}) for order_id in range(1001, 1041)))
total = round(sum(order["total"] for order in orders), 2)
refunded = sum(order["status"] == "refunded" for order in orders)
await submit_answer({"total": total, "refunded": refunded})
