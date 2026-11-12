import asyncio, json

order_ids = [1001, 1500, 1002, 2000]
results = await asyncio.gather(*(get_order({"order_id": order_id}) for order_id in order_ids), return_exceptions=True)
ok, errors = [], {}
for order_id, result in zip(order_ids, results):
    if isinstance(result, BaseException):
        errors[str(order_id)] = str(result)
    else:
        ok.append(json.loads(result)["id"])
await submit_answer({"ok": ok, "errors": errors})
