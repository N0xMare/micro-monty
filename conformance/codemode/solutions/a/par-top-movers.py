import json
import asyncio

syms = ["ACME", "CYDN", "GLBX", "INIT", "OSCP", "STRK", "UMBR", "WAYN"]
quotes = [json.loads(r) for r in await asyncio.gather(*(get_quote({"symbol": s}) for s in syms))]
ranked = sorted(quotes, key=lambda q: (-abs(q["change_pct"]), q["symbol"]))
movers = [{"symbol": q["symbol"], "change_pct": q["change_pct"]} for q in ranked[:3]]
print(movers)
await submit_answer({"movers": movers})
