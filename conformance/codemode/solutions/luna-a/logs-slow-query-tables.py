import asyncio
import json
import re
from collections import defaultdict

services = json.loads(await list_services({}))
texts = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-15"}) for s in services))
stats = defaultdict(lambda: {"count": 0, "max_ms": 0})
pattern = re.compile(r"slow query took (\d+)ms table=([^\s]+)")
for text in texts:
    for line in text.splitlines():
        match = pattern.search(line)
        if match:
            ms, table = int(match.group(1)), match.group(2)
            stats[table]["count"] += 1
            stats[table]["max_ms"] = max(stats[table]["max_ms"], ms)
await submit_answer({"tables": dict(stats)})
