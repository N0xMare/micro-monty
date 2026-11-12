import asyncio
import json

ids = [1001, 1500, 1002, 2000]
results = await asyncio.gather(*(get_order({"order_id": i}) for i in ids), return_exceptions=True)

ok = []
errors = {}
for i, r in zip(ids, results):
    if isinstance(r, BaseException):
        errors[str(i)] = str(r)
    else:
        json.loads(r)
        ok.append(i)

print(ok, errors)
print(await submit_answer({"ok": ok, "errors": errors}))
