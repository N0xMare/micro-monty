import csv, io, json

response = json.loads(await list_orders({"customer_id": 5}))
orders = response["orders"]
cursor = response["next_cursor"]
while cursor is not None:
    response = json.loads(await list_orders({"customer_id": 5, "cursor": cursor}))
    orders.extend(response["orders"])
    cursor = response["next_cursor"]
orders.sort(key=lambda order: (order["order_date"], order["id"]))
buffer = io.StringIO(newline="")
writer = csv.writer(buffer, lineterminator="\n")
writer.writerow(["id", "date", "status", "total"])
writer.writerows([[order["id"], order["order_date"], order["status"], f"{order['total']:.2f}"] for order in orders])
await write_file({"path": "/out/customer5.csv", "content": buffer.getvalue()})
