import asyncio
import json

async def fetch(symbol):
    for attempt in range(4):
        try:
            raw = await get_quote({"symbol": symbol})
        except (TimeoutError, RuntimeError) as error:
            if attempt == 3 or not (isinstance(error, TimeoutError) or "503" in str(error)):
                raise
        else:
            return json.loads(raw)["price"]

symbols = ["ACME", "GLBX", "INIT"]
prices = await asyncio.gather(*(fetch(s) for s in symbols))
await submit_answer({"prices": dict(zip(symbols, prices))})
