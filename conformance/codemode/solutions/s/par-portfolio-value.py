holdings = {"ACME": 120, "GLBX": 300, "INIT": 15, "UMBR": 1000, "WAYN": 40}
symbols = list(holdings)
quotes = await asyncio.gather(*(get_quote({"symbol": s}) for s in symbols))
values = {s: q["price"] * holdings[s] for s, q in zip(symbols, quotes)}
total = round(sum(values.values()), 2)
largest = max(values, key=values.get)
print(values, total, largest)
await submit_answer({"total_usd": total, "largest": largest})
