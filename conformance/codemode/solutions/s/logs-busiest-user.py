import asyncio
import re
from collections import Counter

services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-14"}) for s in services))

line_re = re.compile(r'^(\S+)\s+(DEBUG|INFO|WARN|ERROR)\s+(\S+?)(?:\[\d+\])?:\s?(.*)$')
user_re = re.compile(r'\buser(?:_id)?=(\S+)')
counts = Counter()
samples = []
for svc, text in zip(services, logs):
    for line in text.splitlines():
        m = line_re.match(line)
        if not m:
            continue
        msg = m.group(4)
        if not re.search(r'\bcompleted\b', msg, re.I):
            continue
        u = user_re.search(msg)
        if not u:
            continue
        if len(samples) < 5:
            samples.append(line)
        counts[u.group(1).strip('"\',')] += 1

print("samples:", *samples, sep="\n")
ranked = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))
print(ranked[:5])
user, n = ranked[0]
print(await submit_answer({"user": user, "requests": n}))
