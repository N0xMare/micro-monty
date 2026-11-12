customer = None
page = 1
while True:
    result = await list_customers({"page": page})
    customer = next((c for c in result["customers"] if c["email"] == "ben.kim23@example.com"), None)
    if customer is not None or page >= result["total_pages"]:
        break
    page += 1
if customer is None:
    raise ValueError("Customer not found")
orders = []
args = {"customer_id": customer["id"]}
while True:
    result = await list_orders(args)
    orders.extend(result["orders"])
    if result["next_cursor"] is None:
        break
    args = {"customer_id": customer["id"], "cursor": result["next_cursor"]}
order = max(orders, key=lambda o: (o["order_date"], o["id"]))
action = "none"
if order["status"] == "pending":
    await send_email({"to": customer["email"], "subject": f"Your order {order['id']} is still pending", "body": f"Hello {customer['name']},\nYour order {order['id']} is still pending. Please contact us if you need assistance."})
    action = "emailed"
await submit_answer({"order_id": order["id"], "action": action})
