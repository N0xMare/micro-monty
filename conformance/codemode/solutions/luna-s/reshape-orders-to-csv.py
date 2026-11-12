import csv
import io
orders = []
cursor = None
while True:
    args = {"customer_id": 5}
    if cursor is not None:
        args["cursor"] = cursor
    result = await list_orders(args)
    orders.extend(result["orders"])
    cursor = result["next_cursor"]
    if cursor is None:
        break
orders.sort(key=lambda x: (x["order_date"], x["id"]))
out = io.StringIO(newline="")
writer = csv.writer(out, lineterminator="\n")
writer.writerow(["id", "date", "status", "total"])
for order in orders:
    writer.writerow([order["id"], order["order_date"], order["status"], f"{order['total']:.2f}"])
await write_file({"path": "/out/customer5.csv", "content": out.getvalue()})
