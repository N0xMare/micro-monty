import asyncio
import hashlib
import base64

customers = await asyncio.gather(*(get_customer({"customer_id": i}) for i in (1, 2, 3)))
ids = {str(c["id"]): base64.b32encode(hashlib.sha256(("tenant-42:" + c["email"]).encode("utf-8")).digest()[:10]).decode("ascii").lower() for c in customers}
await submit_answer({"ids": ids})
