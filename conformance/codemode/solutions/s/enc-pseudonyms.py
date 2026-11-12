import asyncio, base64, hashlib

ids = [1, 2, 3]
custs = await asyncio.gather(*(get_customer({"customer_id": i}) for i in ids))
out = {}
for i, c in zip(ids, custs):
    d = hashlib.sha256(f"tenant-42:{c['email']}".encode("utf-8")).digest()[:10]
    out[str(i)] = base64.b32encode(d).decode("ascii").lower()
print(out)
await submit_answer({"ids": out})
