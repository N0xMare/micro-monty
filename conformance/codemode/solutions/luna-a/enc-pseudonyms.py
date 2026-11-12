import base64
import hashlib
import json
customers = [json.loads(await get_customer({"customer_id": i})) for i in (1, 2, 3)]
ids = {}
for c in customers:
    digest = hashlib.sha256(f"tenant-42:{c['email']}".encode("utf-8")).digest()[:10]
    ids[str(c["id"])] = base64.b32encode(digest).decode("ascii").rstrip("=").lower()
await submit_answer({"ids": ids})
