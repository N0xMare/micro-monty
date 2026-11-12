import json

customers = []
page_num = 1
while True:
    page = json.loads(await list_customers({"page": page_num}))
    customers.extend(page["customers"])
    if page_num >= page["total_pages"]:
        break
    page_num += 1
customer = next(c for c in customers if c["email"].lower() == "ben.kim23@example.com")
orders = []
cursor = None
while True:
    args = {"customer_id": customer["id"]}
    if cursor:
        args["cursor"] = cursor
    page = json.loads(await list_orders(args))
    orders.extend(page["orders"])
    cursor = page["next_cursor"]
    if not cursor:
        break
latest = max(orders, key=lambda o: (o["order_date"], o["id"]))
action = "none"
if latest["status"] == "pending":
    await send_email({"to": customer["email"], "subject": f"Your order {latest['id']} is still pending", "body": f"Hello {customer['name'].split()[0]}, your order {latest['id']} is still pending. We will notify you when it ships."})
    action = "emailed"
await submit_answer({"order_id": latest["id"], "action": action})
