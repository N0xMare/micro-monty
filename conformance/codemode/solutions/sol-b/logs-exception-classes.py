import asyncio
import json
import re
from collections import Counter

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-13"}) for service in services))
counts = Counter()
for log in logs:
    counts.update(re.findall(r"unhandled exception in worker: ([^:\s]+):", log))
await submit_answer({"counts": dict(counts)})
