from decimal import Decimal

customer = await get_customer({"customer_id": 7})
orders = []
args = {"customer_id": 7}
while True:
    result = await list_orders(args)
    orders.extend(result["orders"])
    if result["next_cursor"] is None:
        break
    args = {"customer_id": 7, "cursor": result["next_cursor"]}
lines = [f"Statement for {customer['name']} <{customer['email']}>", ""]
for o in sorted(orders, key=lambda o: (o["order_date"], o["id"])):
    lines.append(f"{o['order_date']}  #{o['id']}  {o['status']:<9}  {o['total']:10,.2f}")
total = sum((Decimal(str(o["total"])) for o in orders if o["status"] == "completed"), Decimal(0))
lines.extend(["", f"Completed total: {total:,.2f} USD"])
await write_file({"path": "/out/statement_7.txt", "content": "\n".join(lines)})
