import asyncio
import json
import re
from collections import Counter

services = json.loads(await list_services({}))
texts = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-13"}) for s in services))
counts = Counter()
pattern = re.compile(r"unhandled exception in worker:\s*([A-Za-z_][A-Za-z0-9_.]*)\s*:")
for text in texts:
    for line in text.splitlines():
        match = pattern.search(line)
        if match:
            counts[match.group(1).rsplit(".", 1)[-1]] += 1
await submit_answer({"counts": dict(counts)})
