import asyncio, json

symbols = sorted(["ACME", "GLBX", "INIT", "UMBR", "WAYN", "STRK", "OSCP", "CYDN"])
quotes = [json.loads(s) for s in await asyncio.gather(*(get_quote({"symbol": s}) for s in symbols))]

lines = ["symbol,price,change"]
for sym, q in zip(symbols, quotes):
    change = round(q["change_pct"], 2)
    if change == 0:
        change = 0.0  # avoid "-0.00%"
    lines.append(f"{sym},{q['price']:.2f},{change:+.2f}%")
content = "\n".join(lines) + "\n"

print(content)
print(await write_file({"path": "/out/quotes.csv", "content": content}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"path": "/out/quotes.csv"}))
