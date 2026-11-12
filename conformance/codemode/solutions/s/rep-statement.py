import asyncio

async def all_orders():
    out, cursor = [], None
    while True:
        args = {"customer_id": 7}
        if cursor:
            args["cursor"] = cursor
        r = await list_orders(args)
        out.extend(r["orders"])
        cursor = r["next_cursor"]
        if not cursor:
            return out

customer, orders = await asyncio.gather(get_customer({"customer_id": 7}), all_orders())
orders = [o for o in orders if o["customer_id"] == 7]
orders.sort(key=lambda o: (o["order_date"], o["id"]))

lines = [f"Statement for {customer['name']} <{customer['email']}>", ""]
for o in orders:
    lines.append(f"{o['order_date']}  #{o['id']}  {o['status']:<9}  {o['total']:>10,.2f}")
completed = sum(o["total"] for o in orders if o["status"] == "completed")
lines += ["", f"Completed total: {completed:,.2f} USD"]
content = "\n".join(lines) + "\n"
print(content)

res = await write_file({"path": "/out/statement_7.txt", "content": content})
print(res)
