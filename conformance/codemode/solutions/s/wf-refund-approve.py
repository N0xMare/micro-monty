from datetime import date

ORDER_ID = 1068
order = await get_order({"order_id": ORDER_ID})
customer = await get_customer({"customer_id": order["customer_id"]})
print(order["status"], order["order_date"], customer["email"])

order_date = date.fromisoformat(order["order_date"][:10])
cutoff = date(2026, 2, 13)
is_completed = order["status"] == "completed"
recent = order_date >= cutoff
approved = is_completed and recent

first = customer["name"].split()[0]
if approved:
    subject = f"Refund approved for order {ORDER_ID}"
    body = (
        f"Hello {first},\n\n"
        f"Your refund request for order {ORDER_ID} (placed {order['order_date']}, "
        f"total {order['total']:.2f} {order['currency']}) has been approved. "
        "The refund will be issued to your original payment method.\n\nBest regards"
    )
else:
    reasons = []
    if not is_completed:
        reasons.append(f"the order's status is \"{order['status']}\" rather than completed")
    if not recent:
        reasons.append(f"it was placed on {order['order_date']}, more than 30 days ago (refunds require 2026-02-13 or later)")
    subject = f"Refund declined for order {ORDER_ID}"
    body = (
        f"Hello {first},\n\n"
        f"We're sorry, but your refund request for order {ORDER_ID} has been declined because "
        + " and ".join(reasons)
        + ".\n\nIf you have questions, please reply to this email.\n\nBest regards"
    )

await send_email({"to": customer["email"], "subject": subject, "body": body})
print(subject)

if "submit_answer" in globals():
    await submit_answer({"order_id": ORDER_ID, "approved": approved, "email": customer["email"]})
