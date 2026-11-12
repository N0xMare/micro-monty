import json

res = json.loads(await list_orders({}))
orders = res["orders"]
newest = max(orders, key=lambda o: (o["order_date"], o["id"]))
print(newest)
await submit_answer({"order_id": newest["id"]})
