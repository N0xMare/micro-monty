import asyncio
import json

ids = (1003, 1017, 1029, 1044, 1058, 1071, 1090, 1123, 1150, 1199)
raw_orders = await asyncio.gather(*(get_order({"order_id": order_id}) for order_id in ids))
orders = [json.loads(result) for result in raw_orders]
customer_ids = sorted({order["customer_id"] for order in orders})
raw_customers = await asyncio.gather(*(get_customer({"customer_id": customer_id}) for customer_id in customer_ids))
names = {customer_id: json.loads(result)["name"] for customer_id, result in zip(customer_ids, raw_customers)}
await submit_answer({"customers": {str(order["id"]): names[order["customer_id"]] for order in orders}})
