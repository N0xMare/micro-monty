import json

page = json.loads(await list_orders({}))
orders = page["orders"]
newest = orders[0]
print(newest)
print(await submit_answer({"order_id": newest["id"]}))
