import asyncio, json

symbols = ["ACME", "INIT", "STRK"]
quotes = [json.loads(q) for q in await asyncio.gather(*(get_quote({"symbol": s}) for s in symbols))]
for q in quotes:
    print(q)
weighted = sum(q["price"] * q["change_pct"] for q in quotes) / sum(q["price"] for q in quotes)
result = {"weighted_change_pct": round(weighted, 3)}
print(result)
print(await submit_answer(result))
