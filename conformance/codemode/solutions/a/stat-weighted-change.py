import asyncio, json

symbols = ["ACME", "INIT", "STRK"]
quotes = [json.loads(r) for r in await asyncio.gather(*(get_quote({"symbol": s}) for s in symbols))]
num = sum(q["price"] * q["change_pct"] for q in quotes)
den = sum(q["price"] for q in quotes)

await submit_answer({"weighted_change_pct": round(num / den, 3)})
