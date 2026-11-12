import asyncio
import base64
import hashlib

customers = await asyncio.gather(*(get_customer({"customer_id": customer_id}) for customer_id in (1, 2, 3)))
ids = {}
for customer in customers:
    digest = hashlib.sha256(("tenant-42:" + customer["email"]).encode("utf-8")).digest()[:10]
    ids[str(customer["id"])] = base64.b32encode(digest).decode("ascii").rstrip("=").lower()
await submit_answer({"ids": ids})
