import json
totals = {t: [0.0, 0] for t in ("bronze", "silver", "gold")}
page = 1
while True:
    r = json.loads(await list_customers({"page": page}))
    for c in r["customers"]:
        cursor = None
        while True:
            args = {"customer_id": c["id"], "status": "completed"}
            if cursor: args["cursor"] = cursor
            o = json.loads(await list_orders(args))
            for order in o["orders"]:
                totals[c["tier"]][0] += order["total"]
                totals[c["tier"]][1] += 1
            cursor = o["next_cursor"]
            if not cursor: break
    if page >= r["total_pages"]: break
    page += 1
await submit_answer({"averages": {t: round(s / n, 2) for t, (s, n) in totals.items() if n}})
