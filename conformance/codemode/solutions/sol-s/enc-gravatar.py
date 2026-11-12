import asyncio
import hashlib

customers = await asyncio.gather(*(get_customer({"customer_id": i}) for i in range(1, 6)))
await submit_answer({"hashes": {str(c["id"]): hashlib.md5(c["email"].strip().lower().encode("utf-8")).hexdigest() for c in customers}})
