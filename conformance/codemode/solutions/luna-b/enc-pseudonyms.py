import asyncio, base64, hashlib, json

customers = await asyncio.gather(*(get_customer({"customer_id": i}) for i in (1, 2, 3)))
ids = {}
for i, raw in zip((1, 2, 3), customers):
    email = json.loads(raw)["email"]
    digest = hashlib.sha256(("tenant-42:" + email).encode("utf-8")).digest()[:10]
    ids[str(i)] = base64.b32encode(digest).decode("ascii").rstrip("=").lower()
await submit_answer({"ids": ids})
