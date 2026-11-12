import json
import asyncio
import hashlib

custs = await asyncio.gather(*(get_customer({"customer_id": i}) for i in range(1, 6)))
hashes = {}
for raw in custs:
    c = json.loads(raw)
    hashes[str(c["id"])] = hashlib.md5(c["email"].strip().lower().encode("utf-8")).hexdigest()
print(hashes)
print(await submit_answer({"hashes": hashes}))
