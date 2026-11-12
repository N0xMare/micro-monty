import csv
import io

orders = []
args = {"customer_id": 5}
while True:
    page = await list_orders(args)
    orders.extend(page["orders"])
    if page["next_cursor"] is None:
        break
    args = {"customer_id": 5, "cursor": page["next_cursor"]}
output = io.StringIO(newline="")
writer = csv.writer(output, lineterminator="\n")
writer.writerow(["id", "date", "status", "total"])
for order in sorted(orders, key=lambda o: (o["order_date"], o["id"])):
    writer.writerow([order["id"], order["order_date"], order["status"], f"{order['total']:.2f}"])
await write_file({"path": "/out/customer5.csv", "content": output.getvalue()})
