import asyncio
import json

order_ids = list(range(1001, 1041))
results = await asyncio.gather(*(get_order({"order_id": order_id}) for order_id in order_ids), return_exceptions=True)
total = 0.0
refunded = 0
for result in results:
    if isinstance(result, BaseException):
        continue
    order = json.loads(result)
    total += order["total"]
    refunded += order["status"] == "refunded"
await submit_answer({"total": round(total, 2), "refunded": refunded})
