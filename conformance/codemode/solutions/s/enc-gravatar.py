import asyncio, hashlib

ids = range(1, 6)
custs = await asyncio.gather(*(get_customer({"customer_id": i}) for i in ids))
hashes = {str(i): hashlib.md5(c["email"].strip().lower().encode("utf-8")).hexdigest() for i, c in zip(ids, custs)}
print(hashes)
await submit_answer({"hashes": hashes})
