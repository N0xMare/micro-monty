import asyncio, json

ids = list(range(1001, 1041))
results = await asyncio.gather(*(get_order({"order_id": i}) for i in ids), return_exceptions=True)

orders = {}
failed = []
for i, r in zip(ids, results):
    if isinstance(r, Exception):
        failed.append(i)
    else:
        orders[i] = json.loads(r)

# Retry any failures (e.g. rate limiting) with bounded concurrency.
if failed:
    print("retrying:", failed)
    sem = asyncio.Semaphore(5)
    async def fetch(i):
        async with sem:
            return i, json.loads(await get_order({"order_id": i}))
    for i, o in await asyncio.gather(*(fetch(i) for i in failed)):
        orders[i] = o

currencies = {o["currency"] for o in orders.values()}
if len(currencies) > 1:
    print("WARNING: mixed currencies:", currencies)
total = round(sum(o["total"] for o in orders.values()), 2)
refunded = sum(1 for o in orders.values() if o["status"].lower() == "refunded")
print(len(orders), "orders; total", total, "refunded", refunded)
print(await submit_answer({"total": total, "refunded": refunded}))
