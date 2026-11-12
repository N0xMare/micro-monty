customers = []
page = 1
while True:
    result = await list_customers({"page": page})
    customers.extend(result["customers"])
    if page >= result["total_pages"]: break
    page += 1
customer = next(c for c in customers if c["email"].lower() == "ben.kim23@example.com")
orders = []
cursor = None
while True:
    args = {"customer_id": customer["id"]}
    if cursor is not None: args["cursor"] = cursor
    result = await list_orders(args)
    orders.extend(result["orders"])
    cursor = result["next_cursor"]
    if cursor is None: break
latest = max(orders, key=lambda order: (order["order_date"], order["id"]))
action = "none"
if latest["status"] == "pending":
    await send_email({"to": customer["email"], "subject": f"Your order {latest['id']} is still pending", "body": f"Your order {latest['id']} is still pending. We will update you when it ships."})
    action = "emailed"
await submit_answer({"order_id": latest["id"], "action": action})
