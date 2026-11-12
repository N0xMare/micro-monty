import json, asyncio, hashlib, base64

ids = [1, 2, 3]
results = await asyncio.gather(*(get_customer({"customer_id": i}) for i in ids))

out = {}
for i, raw in zip(ids, results):
    email = json.loads(raw)["email"]
    digest = hashlib.sha256(f"tenant-42:{email}".encode("utf-8")).digest()
    out[str(i)] = base64.b32encode(digest[:10]).decode("ascii").lower()

print(out)
print(await submit_answer({"ids": out}))
