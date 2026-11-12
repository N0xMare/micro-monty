import asyncio
import json

symbols = ["ACME", "CYDN", "GLBX", "INIT", "OSCP", "STRK", "UMBR", "WAYN"]
results = await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in symbols))
quotes = [json.loads(result) for result in results]
movers = [{"symbol": quote["symbol"], "change_pct": quote["change_pct"]} for quote in sorted(quotes, key=lambda quote: (-abs(quote["change_pct"]), quote["symbol"]))[:3]]
await submit_answer({"movers": movers})
