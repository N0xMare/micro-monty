import asyncio
import re

services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-14"}) for s in services))
level_re = re.compile(r'^\S+\s+(DEBUG|INFO|WARN|ERROR)\s')
counts = {}
for svc, text in zip(services, logs):
    n = 0
    for line in text.splitlines():
        m = level_re.match(line)
        if m and m.group(1) == "ERROR":
            n += 1
    counts[svc] = n
print(counts)
print(await submit_answer({"counts": counts}))
