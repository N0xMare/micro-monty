import json

found = None
cursor = None
while found is None:
    args = {"cursor": cursor} if cursor else {}
    res = json.loads(await list_orders(args))
    for o in res["orders"]:
        if o["total"] > 3000:
            found = o
            break
    cursor = res.get("next_cursor")
    if not cursor:
        break
print(found)
if found is not None:
    await submit_answer({"order_id": found["id"]})
