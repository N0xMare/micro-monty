import json
page_number = 1
customer = None
while customer is None:
    page = json.loads(await list_customers({"page": page_number}))
    customer = next((c for c in page["customers"] if c["email"] == "ben.kim23@example.com"), None)
    if customer is not None:
        break
    if page_number >= page["total_pages"]:
        raise RuntimeError("Customer not found")
    page_number += 1
orders = []
args = {"customer_id": customer["id"]}
while True:
    page = json.loads(await list_orders(args))
    orders.extend(page["orders"])
    if page["next_cursor"] is None:
        break
    args["cursor"] = page["next_cursor"]
latest = max(orders, key=lambda order: (order["order_date"], order["id"]))
action = "none"
if latest["status"] == "pending":
    subject = f"Your order {latest['id']} is still pending"
    await send_email({"to": customer["email"], "subject": subject, "body": f"Hi {customer['name']},\n\n{subject}. Please contact us if you need assistance."})
    action = "emailed"
await submit_answer({"order_id": latest["id"], "action": action})
