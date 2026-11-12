import asyncio
import re
from collections import Counter

services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-13"}) for service in services))
counts = Counter()
for log in logs:
    counts.update(re.findall(r'unhandled exception in worker: ([\w.]+):', log))
await submit_answer({"counts": dict(counts)})
