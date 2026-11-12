import asyncio
import re
from collections import Counter

services = __import__("json").loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-14"}) for s in services))
counts = Counter()
for text in logs:
    for line in text.splitlines():
        if "request completed" in line:
            match = re.search(r"\buser=([^\s]+)", line)
            if match:
                counts[match.group(1)] += 1
user, requests = min(counts.items(), key=lambda item: (-item[1], item[0]))
await submit_answer({"user": user, "requests": requests})
