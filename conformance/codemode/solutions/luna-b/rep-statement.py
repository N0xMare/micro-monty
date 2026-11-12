import asyncio, csv, io, json

customer_id = 7
customer_raw, first_raw = await asyncio.gather(get_customer({"customer_id": customer_id}), list_orders({"customer_id": customer_id}))
customer = json.loads(customer_raw)
response = json.loads(first_raw)
orders = response["orders"]
cursor = response["next_cursor"]
while cursor is not None:
    response = json.loads(await list_orders({"customer_id": customer_id, "cursor": cursor}))
    orders.extend(response["orders"])
    cursor = response["next_cursor"]
orders.sort(key=lambda order: (order["order_date"], order["id"]))
lines = [f"Statement for {customer['name']} <{customer['email']}>", ""]
lines.extend(f"{order['order_date']}  #{order['id']}  {order['status']:<9}  {order['total']:>10,.2f}" for order in orders)
lines.extend(["", f"Completed total: {sum(order['total'] for order in orders if order['status'] == 'completed'):,.2f} USD"])
await write_file({"path": "/out/statement_7.txt", "content": "\n".join(lines) + "\n"})
