import asyncio
import json

symbols = ["ACME", "CYDN", "GLBX", "INIT", "OSCP", "STRK", "UMBR", "WAYN"]
responses = await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in symbols))
quotes = [json.loads(response) for response in responses]
movers = sorted(({"symbol": quote["symbol"], "change_pct": quote["change_pct"]} for quote in quotes), key=lambda item: (-abs(item["change_pct"]), item["symbol"]))[:3]
await submit_answer({"movers": movers})
