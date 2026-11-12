import asyncio

order_ids = (1003, 1017, 1029, 1044, 1058, 1071, 1090, 1123, 1150, 1199)
orders = await asyncio.gather(*(get_order({"order_id": order_id}) for order_id in order_ids))
customer_ids = sorted({order["customer_id"] for order in orders})
customers = await asyncio.gather(*(get_customer({"customer_id": customer_id}) for customer_id in customer_ids))
names = {customer_id: customer["name"] for customer_id, customer in zip(customer_ids, customers)}
await submit_answer({"customers": {str(order["id"]): names[order["customer_id"]] for order in orders}})
