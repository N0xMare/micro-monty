import asyncio
import json
import re
from collections import Counter

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-13"}) for service in services))
counts = Counter()
for text in logs:
    for line in text.splitlines():
        match = re.search(r"unhandled exception in worker:\s*([A-Za-z_][\w.]*)\s*:", line)
        if match:
            counts[match.group(1)] += 1
await submit_answer({"counts": dict(counts)})
