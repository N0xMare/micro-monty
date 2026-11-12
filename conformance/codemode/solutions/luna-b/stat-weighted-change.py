import asyncio
import json

symbols = ["ACME", "INIT", "STRK"]
quotes = [json.loads(q) for q in await asyncio.gather(*(get_quote({"symbol": s}) for s in symbols))]
weighted = sum(q["price"] * q["change_pct"] for q in quotes) / sum(q["price"] for q in quotes)
await submit_answer({"weighted_change_pct": round(weighted, 3)})
