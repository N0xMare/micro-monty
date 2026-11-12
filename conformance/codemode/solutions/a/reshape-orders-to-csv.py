import json

orders = {}
cursor = None
while True:
    args = {"customer_id": 5}
    if cursor:
        args["cursor"] = cursor
    page = json.loads(await list_orders(args))
    for o in page["orders"]:
        if o["customer_id"] == 5:
            orders[o["id"]] = o
    cursor = page.get("next_cursor")
    if not cursor:
        break

rows = sorted(orders.values(), key=lambda o: (o["order_date"], o["id"]))
lines = ["id,date,status,total"]
for o in rows:
    lines.append(f'{o["id"]},{o["order_date"]},{o["status"]},{o["total"]:.2f}')
content = "\n".join(lines) + "\n"

print(await write_file({"path": "/out/customer5.csv", "content": content}))
print(len(rows), "orders written")
print(content)
