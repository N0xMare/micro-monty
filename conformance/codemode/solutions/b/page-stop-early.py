import json

found = None
cursor = None
pages = 0
while found is None:
    args = {}
    if cursor:
        args["cursor"] = cursor
    page = json.loads(await list_orders(args))
    pages += 1
    for o in page["orders"]:
        if o["total"] > 3000:
            found = o
            break
    cursor = page.get("next_cursor")
    if found is None and not cursor:
        break

print(pages, found)
if found is not None:
    print(await submit_answer({"order_id": found["id"]}))
