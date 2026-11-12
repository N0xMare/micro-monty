import asyncio, json

symbols = sorted(["ACME", "GLBX", "INIT", "UMBR", "WAYN", "STRK", "OSCP", "CYDN"])
quotes = [json.loads(s) for s in await asyncio.gather(*(get_quote({"symbol": s}) for s in symbols))]
lines = ["symbol,price,change"] + [f"{q['symbol']},{q['price']:.2f},{q['change_pct']:+.2f}%" for q in quotes]
await write_file({"path": "/out/quotes.csv", "content": "\n".join(lines) + "\n"})
