import json

order = json.loads(await get_order({"order_id": 1001}))
customer = json.loads(await get_customer({"customer_id": order["customer_id"]}))
approved = order["status"] == "completed" and order["order_date"] >= "2026-02-13"
decision = "approved" if approved else "declined"
body = f"Hello {customer['name']}, your refund for order 1001 has been approved." if approved else f"Hello {customer['name']}, your refund for order 1001 has been declined because it does not meet the completion and 30-day eligibility requirements."
await send_email({"to": customer["email"], "subject": f"Refund {decision} for order 1001", "body": body})
