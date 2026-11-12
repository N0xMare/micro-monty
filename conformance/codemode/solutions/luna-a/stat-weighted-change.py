import json
import asyncio

symbols = ["ACME", "INIT", "STRK"]
quotes = await asyncio.gather(*(get_quote({"symbol": s}) for s in symbols))
rows = [json.loads(q) for q in quotes]
weighted = sum(float(q["price"]) * float(q["change_pct"]) for q in rows) / sum(float(q["price"]) for q in rows)
await submit_answer({"weighted_change_pct": round(weighted, 3)})
