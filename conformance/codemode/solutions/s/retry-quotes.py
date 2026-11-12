import asyncio

symbols = ["ACME", "GLBX", "INIT"]

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def fetch(sym):
    for attempt in range(4):  # 1 try + 3 retries
        try:
            return await get_quote({"symbol": sym})
        except Exception as e:
            if not transient(e) or attempt == 3:
                raise
            await asyncio.sleep(0.5 * 2 ** attempt)

quotes = await asyncio.gather(*(fetch(s) for s in symbols))
prices = {s: q["price"] for s, q in zip(symbols, quotes)}
print(prices)
print(await submit_answer({"prices": prices}))
