import asyncio
import re
from collections import defaultdict

services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-15"}) for service in services))
stats = defaultdict(lambda: {"count": 0, "max_ms": 0})
pattern = re.compile(r"slow query took (\d+)ms table=([^\s]+)")
for text in logs:
    for line in text.splitlines():
        match = pattern.search(line)
        if match:
            ms, table = int(match.group(1)), match.group(2)
            stats[table]["count"] += 1
            stats[table]["max_ms"] = max(stats[table]["max_ms"], ms)
await submit_answer({"tables": dict(stats)})
