import asyncio
import json
import re

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-15"}) for service in services))
tables = {}
for log in logs:
    for line in log.splitlines():
        if re.match(r"\S+\s+WARN\s", line):
            match = re.search(r"slow query took (\d+)ms table=(\S+)", line)
            if match:
                duration = int(match[1])
                stats = tables.setdefault(match[2], {"count": 0, "max_ms": 0})
                stats["count"] += 1
                stats["max_ms"] = max(stats["max_ms"], duration)
await submit_answer({"tables": tables})
