import csv, io

orders = []
cursor = None
while True:
    args = {"customer_id": 5}
    if cursor:
        args["cursor"] = cursor
    page = await list_orders(args)
    orders.extend(page["orders"])
    cursor = page.get("next_cursor")
    if not cursor:
        break

# dedupe by id and keep only customer 5 (defensive)
by_id = {o["id"]: o for o in orders if o["customer_id"] == 5}
rows = sorted(by_id.values(), key=lambda o: (o["order_date"], o["id"]))

buf = io.StringIO()
w = csv.writer(buf, lineterminator="\n")
w.writerow(["id", "date", "status", "total"])
for o in rows:
    w.writerow([o["id"], o["order_date"], o["status"], f"{o['total']:.2f}"])
content = buf.getvalue()

res = await write_file({"path": "/out/customer5.csv", "content": content})
print(res)
print(f"{len(rows)} orders exported")
print(content)
