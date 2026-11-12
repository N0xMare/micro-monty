import asyncio
import json


def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))


async def call(fn, args, retries=3):
    for attempt in range(retries + 1):
        try:
            return await fn(args)
        except Exception as e:
            if not transient(e) or attempt == retries:
                raise
            await asyncio.sleep(0.2 * 2 ** attempt)


symbols = ["ACME", "GLBX", "INIT"]
raws = await asyncio.gather(*(call(get_quote, {"symbol": s}) for s in symbols))
prices = {s: json.loads(r)["price"] for s, r in zip(symbols, raws)}

print(prices)
print(await submit_answer({"prices": prices}))
