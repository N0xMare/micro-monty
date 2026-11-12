import asyncio, json

symbols = ["ACME", "GLBX", "INIT"]

async def retry_quote(symbol):
    for attempt in range(4):
        try:
            return json.loads(await get_quote({"symbol": symbol}))
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise
            await asyncio.sleep(0)

quotes = await asyncio.gather(*(retry_quote(symbol) for symbol in symbols))
await submit_answer({"prices": {quote["symbol"]: quote["price"] for quote in quotes}})
