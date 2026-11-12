import asyncio, json

async def lookup(customer_id):
    try:
        return json.loads(await get_customer({"customer_id": customer_id}))
    except LookupError:
        return None

ids = [4, 77, 12, 300, 25]
results = await asyncio.gather(*(lookup(i) for i in ids))
await submit_answer({"found": {str(i): r["name"] for i, r in zip(ids, results) if r is not None}, "not_found": [i for i, r in zip(ids, results) if r is None]})
