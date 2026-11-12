import json
from decimal import Decimal

customer = json.loads(await get_customer({"customer_id": 7}))
orders = []
args = {"customer_id": 7}
while True:
    data = json.loads(await list_orders(args), parse_float=Decimal)
    orders.extend(data["orders"])
    if data["next_cursor"] is None:
        break
    args = {"customer_id": 7, "cursor": data["next_cursor"]}
lines = [f"Statement for {customer['name']} <{customer['email']}>", ""]
for o in sorted(orders, key=lambda o: (o["order_date"], o["id"])):
    lines.append(f"{o['order_date']}  #{o['id']}  {o['status']:<9}  {o['total']:10,.2f}")
total = sum(o["total"] for o in orders if o["status"] == "completed")
lines.extend(["", f"Completed total: {total:,.2f} USD"])
await write_file({"path": "/out/statement_7.txt", "content": "\n".join(lines)})
