import asyncio
import json
import re

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-15"}) for s in services))
tables = {}
for log in logs:
    for line in log.splitlines():
        parts = line.split()
        if len(parts) < 2 or parts[1] != "WARN":
            continue
        match = re.search(r'\bslow query took (\d+)ms table=(\S+)', line)
        if match:
            duration, table = match.groups()
            stats = tables.setdefault(table, {"count": 0, "max_ms": 0})
            stats["count"] += 1
            stats["max_ms"] = max(stats["max_ms"], int(duration))
await submit_answer({"tables": tables})
