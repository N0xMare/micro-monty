import json

customer = json.loads(await get_customer({"customer_id": 7}))
orders = []
cursor = None
while True:
    args = {"customer_id": 7}
    if cursor is not None:
        args["cursor"] = cursor
    page = json.loads(await list_orders(args))
    orders.extend(page["orders"])
    cursor = page["next_cursor"]
    if cursor is None:
        break
orders.sort(key=lambda order: (order["order_date"], order["id"]))
lines = [f"Statement for {customer['name']} <{customer['email']}>", ""]
for order in orders:
    lines.append(f"{order['order_date']}  #{order['id']}  {order['status']:<9}  {order['total']:>10,.2f}")
completed_total = sum(order["total"] for order in orders if order["status"] == "completed")
lines.extend(["", f"Completed total: {completed_total:,.2f} USD"])
await write_file({"path": "/out/statement_7.txt", "content": "\n".join(lines) + "\n"})
