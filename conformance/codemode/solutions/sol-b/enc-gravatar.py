import asyncio, json, hashlib
customers = await asyncio.gather(*(get_customer({"customer_id": i}) for i in range(1, 6)))
hashes = {}
for raw in customers:
    customer = json.loads(raw)
    hashes[str(customer["id"])] = hashlib.md5(customer["email"].strip().lower().encode("utf-8")).hexdigest()
await submit_answer({"hashes": hashes})
