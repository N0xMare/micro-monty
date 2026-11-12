import asyncio, json

symbols = sorted(["ACME", "GLBX", "INIT", "UMBR", "WAYN", "STRK", "OSCP", "CYDN"])
quotes = [json.loads(r) for r in await asyncio.gather(
    *(get_quote({"symbol": s}) for s in symbols))]
lines = ["symbol,price,change"]
for s, q in zip(symbols, quotes):
    change = q["change_pct"] + 0.0  # normalize -0.0
    if round(change, 2) == 0:
        change = 0.0
    lines.append(f"{s},{q['price']:.2f},{change:+.2f}%")
content = "\n".join(lines) + "\n"
print(content)
print(await write_file({"path": "/out/quotes.csv", "content": content}))
