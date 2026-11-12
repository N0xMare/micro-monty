import asyncio

async def fetch(symbol):
    for attempt in range(4):
        try:
            return await get_quote({"symbol": symbol})
        except (TimeoutError, RuntimeError) as error:
            if not (isinstance(error, TimeoutError) or "503" in str(error)) or attempt == 3:
                raise

symbols = ["ACME", "GLBX", "INIT"]
quotes = await asyncio.gather(*(fetch(s) for s in symbols))
await submit_answer({"prices": {s: q["price"] for s, q in zip(symbols, quotes)}})
