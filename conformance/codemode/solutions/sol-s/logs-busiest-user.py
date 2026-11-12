import asyncio
import re
from collections import Counter

services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-14"}) for service in services))
counts = Counter()
for log in logs:
    for line in log.splitlines():
        if "request completed" in line:
            match = re.search(r'\buser=(\S+)', line)
            if match:
                counts[match[1]] += 1
user, requests = min(counts.items(), key=lambda x: (-x[1], x[0]))
await submit_answer({"user": user, "requests": requests})
