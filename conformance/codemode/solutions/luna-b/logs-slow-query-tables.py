import asyncio
import json
import re

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-15"}) for service in services))
tables = {}
for text in logs:
    for line in text.splitlines():
        match = re.search(r"slow query took (\d+)ms table=([^\s]+)", line)
        if match:
            table, duration = match.group(2), int(match.group(1))
            stats = tables.setdefault(table, [0, 0])
            stats[0] += 1
            stats[1] = max(stats[1], duration)
await submit_answer({"tables": {table: {"count": values[0], "max_ms": values[1]} for table, values in tables.items()}})
