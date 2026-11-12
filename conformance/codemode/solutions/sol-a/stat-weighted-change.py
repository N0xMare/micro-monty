import asyncio
import json
import math
quotes = [json.loads(raw) for raw in await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in ["ACME", "INIT", "STRK"]))]
change = math.fsum(q["price"] * q["change_pct"] for q in quotes) / math.fsum(q["price"] for q in quotes)
await submit_answer({"weighted_change_pct": round(change, 3)})
