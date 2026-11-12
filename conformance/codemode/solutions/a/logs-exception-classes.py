import asyncio, json, re
from collections import Counter

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-13"}) for s in services))
exc_re = re.compile(r'unhandled exception in worker: ([A-Za-z_][\w.]*):')
counts = Counter()
for text in logs:
    for line in text.splitlines():
        m = exc_re.search(line)
        if m:
            counts[m.group(1)] += 1
counts = dict(sorted(counts.items(), key=lambda kv: (-kv[1], kv[0])))
print(counts)
print(await submit_answer({"counts": counts}))
