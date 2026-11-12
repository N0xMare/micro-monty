import asyncio, json

async def quote(symbol):
    for attempt in range(4):
        try:
            raw = await get_quote({"symbol": symbol})
        except (TimeoutError, RuntimeError) as exc:
            if not (isinstance(exc, TimeoutError) or "503" in str(exc)) or attempt == 3:
                raise
        else:
            return json.loads(raw)

symbols = ["ACME", "GLBX", "INIT"]
quotes = await asyncio.gather(*(quote(s) for s in symbols))
await submit_answer({"prices": {s: q["price"] for s, q in zip(symbols, quotes)}})
