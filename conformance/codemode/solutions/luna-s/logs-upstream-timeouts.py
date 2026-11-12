import asyncio
import re
from collections import Counter

services = await list_services({})
days = ["2026-03-13", "2026-03-14", "2026-03-15"]
jobs = [(service, day) for service in services for day in days]
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": day}) for service, day in jobs))
counts = Counter()
for text in logs:
    for line in text.splitlines():
        match = re.search(r"upstream timeout calling ([^\s]+) after \d+ms", line)
        if match:
            counts[match.group(1)] += 1
await submit_answer({"timeouts": dict(counts)})
