import json

ORDER_ID = 1001
order = json.loads(await get_order({"order_id": ORDER_ID}))
customer = json.loads(await get_customer({"customer_id": order["customer_id"]}))

reasons = []
if order["status"] != "completed":
    reasons.append(f'the order status is "{order["status"]}" (only completed orders can be refunded)')
if order["order_date"] < "2026-02-13":
    reasons.append(f'the order was placed on {order["order_date"]}, more than 30 days ago')
approved = not reasons

first = customer["name"].split()[0] if customer["name"].strip() else "there"
if approved:
    subject = f"Refund approved for order {ORDER_ID}"
    body = (
        f"Hello {first},\n\n"
        f"Your refund request for order {ORDER_ID} (placed {order['order_date']}, total "
        f"{order['total']:.2f} {order['currency']}) has been approved. The amount will be "
        "returned to your original payment method within a few business days.\n\nBest regards"
    )
else:
    subject = f"Refund declined for order {ORDER_ID}"
    body = (
        f"Hello {first},\n\n"
        f"We're sorry, but your refund request for order {ORDER_ID} has been declined because "
        + " and ".join(reasons)
        + ". Refunds are available for completed orders placed within the last 30 days.\n\nBest regards"
    )

await send_email({"to": customer["email"], "subject": subject, "body": body})
result = {"order_id": ORDER_ID, "approved": approved, "decision": "approved" if approved else "declined", "emailed": customer["email"]}
print(json.dumps(result))
if "submit_answer" in globals():
    await submit_answer(result)
