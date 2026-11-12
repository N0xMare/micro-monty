import asyncio, csv, io, json

symbols = ["ACME", "GLBX", "INIT", "UMBR", "WAYN", "STRK", "OSCP", "CYDN"]
quotes = [json.loads(result) for result in await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in symbols))]
buffer = io.StringIO(newline="")
writer = csv.writer(buffer, lineterminator="\n")
writer.writerow(["symbol", "price", "change"])
for quote in sorted(quotes, key=lambda item: item["symbol"]):
    writer.writerow([quote["symbol"], f"{quote['price']:.2f}", f"{quote['change_pct']:+.2f}%"])
await write_file({"path": "/out/quotes.csv", "content": buffer.getvalue()})
