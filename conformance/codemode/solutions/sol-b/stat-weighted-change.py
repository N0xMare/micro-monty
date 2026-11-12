import asyncio
import json
quotes = [json.loads(raw) for raw in await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in ("ACME", "INIT", "STRK")))]
change = sum(quote["price"] * quote["change_pct"] for quote in quotes) / sum(quote["price"] for quote in quotes)
await submit_answer({"weighted_change_pct": round(change, 3)})
