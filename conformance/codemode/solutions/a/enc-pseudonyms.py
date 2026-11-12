import json
import asyncio
import base64
import hashlib

custs = await asyncio.gather(*(get_customer({"customer_id": i}) for i in (1, 2, 3)))
ids = {}
for raw in custs:
    c = json.loads(raw)
    digest = hashlib.sha256(f"tenant-42:{c['email']}".encode("utf-8")).digest()[:10]
    ids[str(c["id"])] = base64.b32encode(digest).decode("ascii").lower()
print(ids)
print(await submit_answer({"ids": ids}))
