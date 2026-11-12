import asyncio

order_ids = [1003, 1017, 1029, 1044, 1058, 1071, 1090, 1123, 1150, 1199]
orders = await asyncio.gather(*(get_order({"order_id": order_id}) for order_id in order_ids))
customer_ids = sorted({order["customer_id"] for order in orders})
customers = await asyncio.gather(*(get_customer({"customer_id": cid}) for cid in customer_ids))
names = {customer["id"]: customer["name"] for customer in customers}
await submit_answer({"customers": {str(order["id"]): names[order["customer_id"]] for order in orders}})
