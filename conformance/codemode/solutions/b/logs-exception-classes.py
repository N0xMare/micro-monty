import asyncio, json, re
from collections import Counter

services = json.loads(await list_services({}))
texts = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-13"}) for s in services))
pat = re.compile(r"unhandled exception in worker: ([\w.]+):")
counts = Counter()
for text in texts:
    for line in text.splitlines():
        m = pat.search(line)
        if m:
            counts[m.group(1)] += 1
print(dict(counts))
print(await submit_answer({"counts": dict(counts)}))
