import asyncio
import json

order_ids = [1003, 1017, 1029, 1044, 1058, 1071, 1090, 1123, 1150, 1199]
order_responses = await asyncio.gather(*(get_order({"order_id": order_id}) for order_id in order_ids))
orders = [json.loads(response) for response in order_responses]
customer_ids = sorted({order["customer_id"] for order in orders})
customer_responses = await asyncio.gather(*(get_customer({"customer_id": customer_id}) for customer_id in customer_ids))
names = {customer["id"]: customer["name"] for customer in map(json.loads, customer_responses)}
customers = {str(order["id"]): names[order["customer_id"]] for order in orders}
await submit_answer({"customers": customers})
