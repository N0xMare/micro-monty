import asyncio

symbols = ["ACME", "CYDN", "GLBX", "INIT", "OSCP", "STRK", "UMBR", "WAYN"]
quotes = await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in symbols))
movers = [{"symbol": quote["symbol"], "change_pct": quote["change_pct"]} for quote in quotes]
movers.sort(key=lambda row: (-abs(row["change_pct"]), row["symbol"]))
await submit_answer({"movers": movers[:3]})
