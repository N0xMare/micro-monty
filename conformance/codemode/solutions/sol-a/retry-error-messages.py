import asyncio

ids = [1001, 1500, 1002, 2000]
results = await asyncio.gather(*(get_order({"order_id": i}) for i in ids), return_exceptions=True)
ok = []
errors = {}
for order_id, result in zip(ids, results):
    if isinstance(result, BaseException):
        errors[str(order_id)] = str(result)
    else:
        ok.append(order_id)
await submit_answer({"ok": ok, "errors": errors})
