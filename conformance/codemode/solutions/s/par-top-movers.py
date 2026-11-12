symbols = ["ACME", "CYDN", "GLBX", "INIT", "OSCP", "STRK", "UMBR", "WAYN"]
quotes = await asyncio.gather(*(get_quote({"symbol": s}) for s in symbols))
ranked = sorted(quotes, key=lambda q: (-abs(q["change_pct"]), q["symbol"]))
movers = [{"symbol": q["symbol"], "change_pct": q["change_pct"]} for q in ranked[:3]]
print(movers)
await submit_answer({"movers": movers})
