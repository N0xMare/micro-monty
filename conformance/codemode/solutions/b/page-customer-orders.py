import json

ids = []
cursor = None
while True:
    args = {"customer_id": 9}
    if cursor:
        args["cursor"] = cursor
    page = json.loads(await list_orders(args))
    ids.extend(o["id"] for o in page["orders"] if o["customer_id"] == 9)
    cursor = page.get("next_cursor")
    if not cursor:
        break

ids = sorted(set(ids))
print(len(ids), ids)
print(await submit_answer({"order_ids": ids}))
