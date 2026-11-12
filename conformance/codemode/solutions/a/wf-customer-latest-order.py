import json

target = "ben.kim23@example.com"
customer, page = None, 1
while customer is None:
    res = json.loads(await list_customers({"page": page}))
    for c in res["customers"]:
        if c["email"].strip().lower() == target:
            customer = c
            break
    if page >= res["total_pages"]:
        break
    page += 1

if customer is None:
    raise ValueError(f"No customer with email {target}")

orders, cursor = [], None
while True:
    args = {"customer_id": customer["id"]}
    if cursor:
        args["cursor"] = cursor
    res = json.loads(await list_orders(args))
    orders.extend(o for o in res["orders"] if o["customer_id"] == customer["id"])
    cursor = res.get("next_cursor")
    if not cursor:
        break

latest = max(orders, key=lambda o: (o["order_date"], o["id"]))
action = "none"
if latest["status"] == "pending":
    await send_email({
        "to": customer["email"],
        "subject": f"Your order {latest['id']} is still pending",
        "body": f"Hi {customer['name']},\n\nYour order {latest['id']} placed on {latest['order_date']} is still pending. We'll let you know as soon as it moves forward.\n\nThank you for your patience.",
    })
    action = "emailed"

await submit_answer({"order_id": latest["id"], "action": action})
