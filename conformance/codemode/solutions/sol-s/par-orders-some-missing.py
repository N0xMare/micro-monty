import asyncio

ids = (1012, 999, 1101, 5000, 1222)
results = await asyncio.gather(*(get_order({"order_id": order_id}) for order_id in ids), return_exceptions=True)
found = {}
missing = []
for order_id, result in zip(ids, results):
    if isinstance(result, Exception):
        missing.append(order_id)
    else:
        found[str(order_id)] = result["total"]
await submit_answer({"found": found, "missing": missing})
