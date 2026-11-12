from datetime import date

order = await get_order({"order_id": 1001})
customer = await get_customer({"customer_id": order["customer_id"]})
age = (date(2026, 3, 15) - date.fromisoformat(order["order_date"][:10])).days
approved = order["status"] == "completed" and 0 <= age <= 30
decision = "approved" if approved else "declined"
if approved:
    explanation = "Your order is completed and was placed within the last 30 days, so your refund request is approved."
else:
    explanation = f"Refunds require a completed order placed within the last 30 days. Your order's status is {order['status']} and it was placed on {order['order_date']}, so your request is declined."
await send_email({"to": customer["email"], "subject": f"Refund {decision} for order 1001", "body": f"Hello {customer['name']},\n{explanation}"})
