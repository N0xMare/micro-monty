import asyncio, json

holdings = {"ACME": 120, "GLBX": 300, "INIT": 15, "UMBR": 1000, "WAYN": 40}
syms = list(holdings)
results = await asyncio.gather(*(get_quote({"symbol": s}) for s in syms))
values = {s: holdings[s] * json.loads(r)["price"] for s, r in zip(syms, results)}
total = round(sum(values.values()), 2)
largest = max(values, key=values.get)
print(values)
print(await submit_answer({"total_usd": total, "largest": largest}))
