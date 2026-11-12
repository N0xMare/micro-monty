import asyncio, json

async def all_orders():
    out, cursor = [], None
    while True:
        args = {"customer_id": 7}
        if cursor:
            args["cursor"] = cursor
        res = json.loads(await list_orders(args))
        out.extend(res["orders"])
        cursor = res["next_cursor"]
        if not cursor:
            return out

cust_raw, orders = await asyncio.gather(get_customer({"customer_id": 7}), all_orders())
cust = json.loads(cust_raw)
orders = sorted((o for o in orders if o["customer_id"] == 7), key=lambda o: (o["order_date"], o["id"]))

lines = [f"Statement for {cust['name']} <{cust['email']}>", ""]
for o in orders:
    lines.append(f"{o['order_date']}  #{o['id']}  {o['status']:<9}  {o['total']:>10,.2f}")
completed = round(sum(o["total"] for o in orders if o["status"] == "completed"), 2)
lines += ["", f"Completed total: {completed:,.2f} USD"]
content = "\n".join(lines) + "\n"

print(content)
print(await write_file({"path": "/out/statement_7.txt", "content": content}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"path": "/out/statement_7.txt"}))
