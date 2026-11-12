import asyncio
import json

symbols = ["ACME", "CYDN", "GLBX", "INIT", "OSCP", "STRK", "UMBR", "WAYN"]
results = await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in symbols))
quotes = [json.loads(result) for result in results]
quotes.sort(key=lambda quote: (-abs(quote["change_pct"]), quote["symbol"]))
await submit_answer({"movers": [{"symbol": quote["symbol"], "change_pct": quote["change_pct"]} for quote in quotes[:3]]})
