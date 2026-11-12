import asyncio
import base64
import hashlib
import json

results = await asyncio.gather(*(get_customer({"customer_id": i}) for i in (1, 2, 3)))
ids = {}
for result in results:
    customer = json.loads(result)
    digest = hashlib.sha256(("tenant-42:" + customer["email"]).encode("utf-8")).digest()[:10]
    ids[str(customer["id"])] = base64.b32encode(digest).decode("ascii").lower()
await submit_answer({"ids": ids})
