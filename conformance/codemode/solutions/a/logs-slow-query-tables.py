import asyncio, json, re

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-15"}) for s in services))
slow_re = re.compile(r'slow query took (\d+)ms table=(\S+)')
tables = {}
for text in logs:
    for line in text.splitlines():
        m = slow_re.search(line)
        if not m:
            continue
        ms, table = int(m.group(1)), m.group(2)
        t = tables.setdefault(table, {"count": 0, "max_ms": 0})
        t["count"] += 1
        t["max_ms"] = max(t["max_ms"], ms)
tables = dict(sorted(tables.items()))
print(tables)
print(await submit_answer({"tables": tables}))
