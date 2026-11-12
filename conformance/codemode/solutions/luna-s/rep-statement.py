customer = await get_customer({"customer_id": 7})
orders = []
cursor = None
while True:
    args = {"customer_id": 7}
    if cursor is not None:
        args["cursor"] = cursor
    page = await list_orders(args)
    orders.extend(page["orders"])
    cursor = page["next_cursor"]
    if cursor is None:
        break
orders.sort(key=lambda x: (x["order_date"], x["id"]))
lines = [f"Statement for {customer['name']} <{customer['email']}>", ""]
for x in orders:
    lines.append(f"{x['order_date']}  #{x['id']}  {x['status']:<9}  {x['total']:>10,.2f}")
completed = sum(x["total"] for x in orders if x["status"] == "completed")
lines.extend(["", f"Completed total: {completed:,.2f} USD"])
await write_file({"path": "/out/statement_7.txt", "content": "\n".join(lines) + "\n"})
