import asyncio
import json

order_ids = [1012, 999, 1101, 5000, 1222]
results = await asyncio.gather(*(get_order({"order_id": order_id}) for order_id in order_ids), return_exceptions=True)
found = {}
missing = []
for order_id, result in zip(order_ids, results):
    if isinstance(result, BaseException):
        missing.append(order_id)
    else:
        found[str(order_id)] = json.loads(result)["total"]
await submit_answer({"found": found, "missing": missing})
