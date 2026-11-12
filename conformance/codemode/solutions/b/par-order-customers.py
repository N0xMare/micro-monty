import asyncio, json

order_ids = [1003, 1017, 1029, 1044, 1058, 1071, 1090, 1123, 1150, 1199]
orders = [json.loads(r) for r in await asyncio.gather(*(get_order({"order_id": i}) for i in order_ids))]
cust_ids = sorted({o["customer_id"] for o in orders})
custs = await asyncio.gather(*(get_customer({"customer_id": c}) for c in cust_ids))
names = {c: json.loads(r)["name"] for c, r in zip(cust_ids, custs)}
customers = {str(i): names[o["customer_id"]] for i, o in zip(order_ids, orders)}
print(customers)
print(await submit_answer({"customers": customers}))
