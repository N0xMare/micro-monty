import asyncio
import json

shares = {"ACME": 120, "GLBX": 300, "INIT": 15, "UMBR": 1000, "WAYN": 40}
symbols = list(shares)
results = await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in symbols))
values = {symbol: shares[symbol] * json.loads(result)["price"] for symbol, result in zip(symbols, results)}
await submit_answer({"total_usd": round(sum(values.values()), 2), "largest": max(values, key=values.get)})
