import asyncio
import json
import re
from collections import Counter

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-13"}) for s in services))
counts = Counter()
for log in logs:
    for line in log.splitlines():
        match = re.search(r'unhandled exception in worker: ([^:\s]+):', line)
        if match:
            counts[match[1]] += 1
await submit_answer({"counts": dict(counts)})
