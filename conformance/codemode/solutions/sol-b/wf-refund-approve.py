import json
from datetime import date
order = json.loads(await get_order({"order_id": 1068}))
customer = json.loads(await get_customer({"customer_id": order["customer_id"]}))
age = (date(2026, 3, 15) - date.fromisoformat(order["order_date"][:10])).days
approved = order["status"] == "completed" and 0 <= age <= 30
decision = "approved" if approved else "declined"
reason = "Your order is completed and was placed within the last 30 days." if approved else "Refunds require a completed order placed within the last 30 days; this order does not meet those requirements."
await send_email({"to": customer["email"], "subject": f"Refund {decision} for order 1068", "body": f"Hello {customer['name']},\nYour refund request for order 1068 is {decision}. {reason}"})
