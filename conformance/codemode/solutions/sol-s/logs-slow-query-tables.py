import asyncio
import re

services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-15"}) for service in services))
tables = {}
for log in logs:
    for line in log.splitlines():
        if re.match(r'^\S+\s+WARN\s+', line):
            match = re.search(r'slow query took (\d+)ms table=(\S+)', line)
            if match:
                entry = tables.setdefault(match[2], {"count": 0, "max_ms": 0})
                entry["count"] += 1
                entry["max_ms"] = max(entry["max_ms"], int(match[1]))
await submit_answer({"tables": tables})
