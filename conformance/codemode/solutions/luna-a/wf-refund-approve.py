import json

order = json.loads(await get_order({"order_id": 1068}))
customer = json.loads(await get_customer({"customer_id": order["customer_id"]}))
approved = order["status"] == "completed" and order["order_date"] >= "2026-02-13"
if approved:
    subject = "Refund approved for order 1068"
    body = "Your order meets the refund requirements. Your refund has been approved."
else:
    subject = "Refund declined for order 1068"
    body = "Your order does not meet the refund requirements because it is not completed or is more than 30 days old."
await send_email({"to": customer["email"], "subject": subject, "body": body})
