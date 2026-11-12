import asyncio, json

cust_raw = get_customer({"customer_id": 7})
orders, cursor = [], None
while True:
    args = {"customer_id": 7}
    if cursor:
        args["cursor"] = cursor
    page = json.loads(await list_orders(args))
    orders += page["orders"]
    cursor = page["next_cursor"]
    if not cursor:
        break
cust = json.loads(await cust_raw)

orders.sort(key=lambda o: (o["order_date"], o["id"]))
lines = [f"Statement for {cust['name']} <{cust['email']}>", ""]
for o in orders:
    lines.append(f"{o['order_date']}  #{o['id']}  {o['status']:<9}  {o['total']:>10,.2f}")
completed = sum(o["total"] for o in orders if o["status"] == "completed")
lines += ["", f"Completed total: {completed:,.2f} USD"]
content = "\n".join(lines) + "\n"
print(content)
print(await write_file({"path": "/out/statement_7.txt", "content": content}))
