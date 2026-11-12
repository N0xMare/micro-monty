import json
import asyncio

holdings = {"ACME": 120, "GLBX": 300, "INIT": 15, "UMBR": 1000, "WAYN": 40}
syms = list(holdings)
quotes = [json.loads(r) for r in await asyncio.gather(*(get_quote({"symbol": s}) for s in syms))]
values = {s: holdings[s] * q["price"] for s, q in zip(syms, quotes)}
print(values, {q["symbol"]: q["currency"] for q in quotes})
total = round(sum(values.values()), 2)
largest = max(values, key=values.get)
await submit_answer({"total_usd": total, "largest": largest})
