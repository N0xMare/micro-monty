import asyncio, json, re

services = json.loads(await list_services({}))
texts = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-15"}) for s in services))
pat = re.compile(r"slow query took (\d+)ms table=(\S+)")
tables = {}
for text in texts:
    for line in text.splitlines():
        m = pat.search(line)
        if m:
            ms, table = int(m.group(1)), m.group(2)
            t = tables.setdefault(table, {"count": 0, "max_ms": 0})
            t["count"] += 1
            t["max_ms"] = max(t["max_ms"], ms)
print(tables)
print(await submit_answer({"tables": tables}))
