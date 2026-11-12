import asyncio
import json
import re
from collections import Counter

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": date}) for s in services for date in ["2026-03-13", "2026-03-14", "2026-03-15"]))
counts = Counter()
for log in logs:
    for line in log.splitlines():
        match = re.search(r'upstream timeout calling (\S+) after \d+ms', line)
        if match:
            counts[match[1]] += 1
await submit_answer({"timeouts": dict(counts)})
