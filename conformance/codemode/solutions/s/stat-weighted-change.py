import asyncio

symbols = ["ACME", "INIT", "STRK"]
quotes = await asyncio.gather(*(get_quote({"symbol": s}) for s in symbols))
num = sum(q["price"] * q["change_pct"] for q in quotes)
den = sum(q["price"] for q in quotes)
w = round(num / den, 3)
print(quotes, w)
await submit_answer({"weighted_change_pct": w})
