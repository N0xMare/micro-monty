import json

customers, page_no = [], 1
while True:
    page = json.loads(await list_customers({"page": page_no}))
    customers.extend(page["customers"])
    if page_no >= page["total_pages"]: break
    page_no += 1
customer = next(c for c in customers if c["email"].lower() == "ben.kim23@example.com")
orders, cursor = [], None
while True:
    args = {"customer_id": customer["id"]}
    if cursor: args["cursor"] = cursor
    page = json.loads(await list_orders(args))
    orders.extend(page["orders"])
    cursor = page["next_cursor"]
    if not cursor: break
order = max(orders, key=lambda o: (o["order_date"], o["id"]))
emailed = order["status"] == "pending"
if emailed:
    await send_email({"to": customer["email"], "subject": f"Your order {order['id']} is still pending", "body": f"Hello {customer['name']}, your order {order['id']} is still pending. Please contact us if you need help."})
await submit_answer({"order_id": order["id"], "action": "emailed" if emailed else "none"})
