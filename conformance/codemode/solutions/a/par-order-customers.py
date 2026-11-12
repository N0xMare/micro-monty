import json
import asyncio

order_ids = [1003, 1017, 1029, 1044, 1058, 1071, 1090, 1123, 1150, 1199]
orders = [json.loads(r) for r in await asyncio.gather(*(get_order({"order_id": i}) for i in order_ids))]
cust_ids = sorted({o["customer_id"] for o in orders})
custs = [json.loads(r) for r in await asyncio.gather(*(get_customer({"customer_id": c}) for c in cust_ids))]
names = {c["id"]: c["name"] for c in custs}
customers = {str(oid): names[o["customer_id"]] for oid, o in zip(order_ids, orders)}
print(customers)
await submit_answer({"customers": customers})
