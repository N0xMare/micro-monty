import asyncio
import json
import re
from collections import Counter

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-14"}) for s in services))
counts = Counter()
for log in logs:
    for line in log.splitlines():
        if "request completed" in line.lower():
            match = re.search(r'\buser=(\S+)', line)
            if match:
                counts[match[1]] += 1
user, requests = min(counts.items(), key=lambda item: (-item[1], item[0]))
await submit_answer({"user": user, "requests": requests})
