import json, csv, io

orders = []
args = {"customer_id": 5}
while True:
    data = json.loads(await list_orders(args))
    orders.extend(data["orders"])
    if data["next_cursor"] is None:
        break
    args = {"customer_id": 5, "cursor": data["next_cursor"]}
out = io.StringIO(newline="")
writer = csv.writer(out, lineterminator="\n")
writer.writerow(["id", "date", "status", "total"])
for o in sorted(orders, key=lambda o: (o["order_date"], o["id"])):
    writer.writerow([o["id"], o["order_date"], o["status"], f"{o['total']:.2f}"])
await write_file({"path": "/out/customer5.csv", "content": out.getvalue()})
