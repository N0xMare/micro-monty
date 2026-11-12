import asyncio, json

syms = ["ACME", "CYDN", "GLBX", "INIT", "OSCP", "STRK", "UMBR", "WAYN"]
results = await asyncio.gather(*(get_quote({"symbol": s}) for s in syms))
changes = {s: json.loads(r)["change_pct"] for s, r in zip(syms, results)}
top = sorted(changes.items(), key=lambda kv: (-abs(kv[1]), kv[0]))[:3]
movers = [{"symbol": s, "change_pct": c} for s, c in top]
print(changes)
print(await submit_answer({"movers": movers}))
