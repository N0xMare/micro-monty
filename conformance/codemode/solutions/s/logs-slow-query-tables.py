import asyncio
import re

services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-15"}) for s in services))
level_re = re.compile(r'^\S+\s+(DEBUG|INFO|WARN|ERROR)\s')
slow_re = re.compile(r'slow query took (\d+)\s*ms\s+table=(\S+)')
tables = {}
for text in logs:
    for line in text.splitlines():
        lm = level_re.match(line)
        if not lm or lm.group(1) != "WARN":
            continue
        m = slow_re.search(line)
        if not m:
            continue
        ms, table = int(m.group(1)), m.group(2).strip('"\',')
        t = tables.setdefault(table, {"count": 0, "max_ms": 0})
        t["count"] += 1
        t["max_ms"] = max(t["max_ms"], ms)
tables = dict(sorted(tables.items()))
print(tables)
print(await submit_answer({"tables": tables}))
