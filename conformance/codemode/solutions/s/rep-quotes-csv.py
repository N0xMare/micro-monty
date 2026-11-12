import asyncio

symbols = sorted(["ACME", "GLBX", "INIT", "UMBR", "WAYN", "STRK", "OSCP", "CYDN"])
quotes = await asyncio.gather(*(get_quote({"symbol": s}) for s in symbols))

lines = ["symbol,price,change"]
for s, q in zip(symbols, quotes):
    lines.append(f"{s},{q['price']:.2f},{q['change_pct']:+.2f}%")
content = "\n".join(lines) + "\n"
print(content)

res = await write_file({"path": "/out/quotes.csv", "content": content})
print(res)
