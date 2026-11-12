import asyncio, hashlib, json

customers = await asyncio.gather(*(get_customer({"customer_id": i}) for i in range(1, 6)))
hashes = {str(i): hashlib.md5(json.loads(raw)["email"].strip().lower().encode("utf-8")).hexdigest() for i, raw in zip(range(1, 6), customers)}
await submit_answer({"hashes": hashes})
