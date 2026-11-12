import asyncio

quotes = await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in ("ACME", "INIT", "STRK")))
weighted = sum(q["price"] * q["change_pct"] for q in quotes) / sum(q["price"] for q in quotes)
await submit_answer({"weighted_change_pct": round(weighted, 3)})
