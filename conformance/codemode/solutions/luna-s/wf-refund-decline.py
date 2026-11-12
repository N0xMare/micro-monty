order = await get_order({"order_id": 1001})
customer = await get_customer({"customer_id": order["customer_id"]})
approved = order["status"] == "completed" and order["order_date"] >= "2026-02-13"
decision = "approved" if approved else "declined"
reason = "The order is completed and within the 30-day refund window." if approved else "The order does not meet the completed-status and 30-day eligibility requirements."
await send_email({"to": customer["email"], "subject": f"Refund {decision} for order 1001", "body": reason})
