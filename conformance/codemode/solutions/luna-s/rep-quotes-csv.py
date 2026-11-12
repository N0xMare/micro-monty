import asyncio
import csv
import io
symbols = ["ACME", "GLBX", "INIT", "UMBR", "WAYN", "STRK", "OSCP", "CYDN"]
quotes = await asyncio.gather(*(get_quote({"symbol": s}) for s in symbols))
out = io.StringIO(newline="")
writer = csv.writer(out, lineterminator="\n")
writer.writerow(["symbol", "price", "change"])
for q in sorted(quotes, key=lambda x: x["symbol"]):
    writer.writerow([q["symbol"], f"{q['price']:.2f}", f"{q['change_pct']:+.2f}%"])
await write_file({"path": "/out/quotes.csv", "content": out.getvalue()})
