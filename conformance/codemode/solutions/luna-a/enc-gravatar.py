import asyncio
import hashlib
import json
raw = await asyncio.gather(*(get_customer({"customer_id": i}) for i in range(1, 6)))
customers = [json.loads(x) for x in raw]
hashes = {str(c["id"]): hashlib.md5(c["email"].strip().lower().encode("utf-8")).hexdigest() for c in customers}
await submit_answer({"hashes": hashes})
