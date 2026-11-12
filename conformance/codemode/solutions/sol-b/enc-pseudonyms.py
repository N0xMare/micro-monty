import asyncio, json, hashlib, base64
raw_customers = await asyncio.gather(*(get_customer({"customer_id": i}) for i in (1, 2, 3)))
ids = {}
for raw in raw_customers:
    c = json.loads(raw)
    digest = hashlib.sha256(("tenant-42:" + c["email"]).encode("utf-8")).digest()[:10]
    ids[str(c["id"])] = base64.b32encode(digest).decode("ascii").lower()
await submit_answer({"ids": ids})
