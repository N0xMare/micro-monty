import asyncio, json

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def with_retry(fn, args, retries=3):
    for attempt in range(retries + 1):
        try:
            return await fn(args)
        except Exception as e:
            if not transient(e) or attempt == retries:
                raise
            await asyncio.sleep(0.2 * 2 ** attempt)

symbols = ["ACME", "GLBX", "INIT"]
results = await asyncio.gather(*[with_retry(get_quote, {"symbol": s}) for s in symbols])
prices = {s: json.loads(r)["price"] for s, r in zip(symbols, results)}
print(prices)
print(await submit_answer({"prices": prices}))
