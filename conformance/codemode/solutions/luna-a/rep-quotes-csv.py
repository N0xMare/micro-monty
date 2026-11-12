import asyncio
import csv
import io
import json

symbols = ["ACME", "GLBX", "INIT", "UMBR", "WAYN", "STRK", "OSCP", "CYDN"]
results = await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in symbols))
quotes = sorted((json.loads(result) for result in results), key=lambda quote: quote["symbol"])
output = io.StringIO(newline="")
writer = csv.writer(output, lineterminator="\n")
writer.writerow(["symbol", "price", "change"])
for quote in quotes:
    writer.writerow([quote["symbol"], f"{quote['price']:.2f}", f"{quote['change_pct']:+.2f}%"])
await write_file({"path": "/out/quotes.csv", "content": output.getvalue()})
