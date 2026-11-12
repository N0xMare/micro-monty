import asyncio

ids = [1001, 1500, 1002, 2000]
results = await asyncio.gather(*(get_order({"order_id": i}) for i in ids), return_exceptions=True)
ok = []
errors = {}
for i, result in zip(ids, results):
    if isinstance(result, Exception):
        errors[str(i)] = str(result)
    else:
        ok.append(i)
await submit_answer({"ok": ok, "errors": errors})
