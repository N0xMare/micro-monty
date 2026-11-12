import asyncio
import json
import re
from collections import Counter

services = json.loads(await list_services({}))
days = ["2026-03-13", "2026-03-14", "2026-03-15"]
texts = await asyncio.gather(*(fetch_logs({"service": service, "date": day}) for service in services for day in days))
counts = Counter()
for text in texts:
    for line in text.splitlines():
        match = re.search(r"upstream timeout calling\s+(\S+)\s+after\s+\d+ms", line)
        if match:
            counts[match.group(1)] += 1
await submit_answer({"timeouts": dict(counts)})
