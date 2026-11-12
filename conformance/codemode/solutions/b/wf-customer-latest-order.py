import json

target = "ben.kim23@example.com"
customer, page = None, 1
while customer is None:
    res = json.loads(await list_customers({"page": page}))
    customer = next((c for c in res["customers"] if c["email"].strip().lower() == target), None)
    if page >= res["total_pages"]:
        break
    page += 1
if customer is None:
    raise RuntimeError(f"no customer with email {target}")
print("customer:", customer)

orders, cursor = [], None
while True:
    args = {"customer_id": customer["id"]}
    if cursor:
        args["cursor"] = cursor
    res = json.loads(await list_orders(args))
    orders.extend(res["orders"])
    cursor = res.get("next_cursor")
    if not cursor:
        break
orders = [o for o in orders if o["customer_id"] == customer["id"]]
latest = max(orders, key=lambda o: (o["order_date"], o["id"]))
print("latest order:", latest)

action = "none"
if latest["status"] == "pending":
    first = customer["name"].split()[0]
    await send_email({
        "to": customer["email"],
        "subject": f"Your order {latest['id']} is still pending",
        "body": f"Hello {first},\n\nYour order {latest['id']} placed on {latest['order_date']} is still pending. We'll let you know as soon as it moves forward.\n\nThank you for your patience.",
    })
    action = "emailed"

print(await submit_answer({"order_id": latest["id"], "action": action}))
