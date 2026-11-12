import json, asyncio, hashlib

ids = [1, 2, 3, 4, 5]
results = await asyncio.gather(*(get_customer({"customer_id": i}) for i in ids))

hashes = {}
for i, raw in zip(ids, results):
    email = json.loads(raw)["email"]
    hashes[str(i)] = hashlib.md5(email.strip().lower().encode("utf-8")).hexdigest()

print(hashes)
print(await submit_answer({"hashes": hashes}))
