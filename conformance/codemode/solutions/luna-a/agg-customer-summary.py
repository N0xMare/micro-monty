import json
c = json.loads(await get_customer({"customer_id": 17}))
counts, spent, cursor = {}, 0.0, None
while True:
    args = {"customer_id": 17}
    if cursor: args["cursor"] = cursor
    r = json.loads(await list_orders(args))
    for o in r["orders"]:
        counts[o["status"]] = counts.get(o["status"], 0) + 1
        if o["status"] == "completed": spent += o["total"]
    cursor = r["next_cursor"]
    if not cursor: break
await submit_answer({"name": c["name"], "counts": counts, "completed_total": round(spent, 2)})
