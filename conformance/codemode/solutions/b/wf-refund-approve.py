import json

ORDER_ID = 1068
order = json.loads(await get_order({"order_id": ORDER_ID}))
customer = json.loads(await get_customer({"customer_id": order["customer_id"]}))
print(order["status"], order["order_date"], customer["email"])

approved = order["status"] == "completed" and order["order_date"][:10] >= "2026-02-13"
first = customer["name"].split()[0]
if approved:
    subject = f"Refund approved for order {ORDER_ID}"
    body = f"Hello {first},\n\nYour refund request for order {ORDER_ID} ({order['total']:.2f} {order['currency']}) has been approved. The amount will be returned to your original payment method shortly."
else:
    reasons = []
    if order["status"] != "completed":
        reasons.append(f"the order status is \"{order['status']}\" rather than completed")
    if order["order_date"][:10] < "2026-02-13":
        reasons.append(f"it was placed on {order['order_date'][:10]}, more than 30 days ago")
    subject = f"Refund declined for order {ORDER_ID}"
    body = f"Hello {first},\n\nWe're sorry, but your refund request for order {ORDER_ID} has been declined because {' and '.join(reasons)}. Refunds are available for completed orders placed within the last 30 days."

print(await send_email({"to": customer["email"], "subject": subject, "body": body}))
print({"order_id": ORDER_ID, "approved": approved, "emailed": customer["email"]})
# submit_answer is not in this cell's tool catalog, so the result is reported via print only.
