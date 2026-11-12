async def __codemode_main():
    global action, args, c, cursor, customer, latest, orders, page, res, target_email
    target_email = "ben.kim23@example.com"
    customer, page = None, 1
    while customer is None:
        res = await list_customers({"page": page})
        for c in res["customers"]:
            if c["email"].strip().lower() == target_email:
                customer = c
                break
        if page >= res["total_pages"]:
            break
        page += 1
    if customer is None:
        raise RuntimeError("customer not found")

    orders, cursor = [], None
    while True:
        args = {"customer_id": customer["id"]}
        if cursor:
            args["cursor"] = cursor
        res = await list_orders(args)
        orders.extend(res["orders"])
        cursor = res.get("next_cursor")
        if not cursor:
            break

    latest = max(orders, key=lambda o: (o["order_date"], o["id"]))
    print(customer, latest)
    action = "none"
    if latest["status"] == "pending":
        await send_email({
            "to": customer["email"],
            "subject": f"Your order {latest['id']} is still pending",
            "body": f"Hello {customer['name']},\n\nYour order {latest['id']} placed on {latest['order_date']} is still pending. We'll let you know as soon as it moves forward.\n\nThank you for your patience.",
        })
        action = "emailed"
    await submit_answer({"order_id": latest["id"], "action": action})
    pass
import asyncio as __codemode_asyncio
__codemode_asyncio.run(__codemode_main())
