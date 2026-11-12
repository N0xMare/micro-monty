import asyncio
async def quote(symbol):
    for attempt in range(4):
        try:
            return await get_quote({"symbol": symbol})
        except (TimeoutError, RuntimeError) as error:
            if isinstance(error, RuntimeError) and "503" not in str(error):
                raise
            if attempt == 3:
                raise
symbols = ["ACME", "GLBX", "INIT"]
results = await asyncio.gather(*(quote(symbol) for symbol in symbols))
await submit_answer({"prices": {item["symbol"]: item["price"] for item in results}})
