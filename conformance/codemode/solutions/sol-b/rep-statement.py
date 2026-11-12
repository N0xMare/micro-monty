import asyncio, json

async def all_orders():
    orders = []
    args = {"customer_id": 7}
    while True:
        data = json.loads(await list_orders(args))
        orders.extend(data["orders"])
        if data["next_cursor"] is None:
            return orders
        args = {"customer_id": 7, "cursor": data["next_cursor"]}

customer_raw, orders = await asyncio.gather(get_customer({"customer_id": 7}), all_orders())
customer = json.loads(customer_raw)
lines = [f"Statement for {customer['name']} <{customer['email']}>", ""]
for o in sorted(orders, key=lambda o: (o["order_date"], o["id"])):
    lines.append(f"{o['order_date']}  #{o['id']}  {o['status']:<9}  {o['total']:10,.2f}")
total = sum(o["total"] for o in orders if o["status"] == "completed")
lines.extend(["", f"Completed total: {total:,.2f} USD"])
await write_file({"path": "/out/statement_7.txt", "content": "\n".join(lines)})
