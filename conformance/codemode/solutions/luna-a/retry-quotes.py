import asyncio
import json

async def quote_with_retries(symbol):
    for attempt in range(4):
        try:
            return json.loads(await get_quote({"symbol": symbol}))
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise

symbols = ["ACME", "GLBX", "INIT"]
quotes = await asyncio.gather(*(quote_with_retries(symbol) for symbol in symbols))
await submit_answer({"prices": {quote["symbol"]: quote["price"] for quote in quotes}})
