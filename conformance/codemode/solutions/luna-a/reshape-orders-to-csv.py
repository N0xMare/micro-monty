import csv
import io
import json

orders = []
cursor = None
while True:
    args = {"customer_id": 5}
    if cursor is not None:
        args["cursor"] = cursor
    page = json.loads(await list_orders(args))
    orders.extend(page["orders"])
    cursor = page["next_cursor"]
    if cursor is None:
        break
orders.sort(key=lambda order: (order["order_date"], order["id"]))
output = io.StringIO(newline="")
writer = csv.writer(output, lineterminator="\n")
writer.writerow(["id", "date", "status", "total"])
for order in orders:
    writer.writerow([order["id"], order["order_date"], order["status"], f"{order['total']:.2f}"])
await write_file({"path": "/out/customer5.csv", "content": output.getvalue()})
