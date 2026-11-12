import json
import asyncio

ids = [1012, 999, 1101, 5000, 1222]
results = await asyncio.gather(*(get_order({"order_id": i}) for i in ids), return_exceptions=True)
found = {}
missing = []
for i, r in zip(ids, results):
    if isinstance(r, Exception):
        print(i, "error:", r)
        missing.append(i)
    else:
        found[str(i)] = json.loads(r)["total"]
print(found, missing)
await submit_answer({"found": found, "missing": missing})
