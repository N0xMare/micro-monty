import asyncio, json, re
from collections import Counter

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-14"}) for s in services))

line_re = re.compile(r'^(\S+)\s+(DEBUG|INFO|WARN|ERROR)\s*(\S+?)\[(\d+)\]:\s(.*)$')
user_re = re.compile(r'\buser(?:_id)?=(u\d+)\b')
counts = Counter()
samples = []
for svc, text in zip(services, logs):
    for line in text.splitlines():
        m = line_re.match(line)
        if not m:
            continue
        msg = m.group(5)
        if "complet" not in msg.lower():
            continue
        if len(samples) < 5:
            samples.append(line)
        u = user_re.search(msg)
        if u:
            counts[u.group(1)] += 1

print("sample completed lines:", *samples, sep="\n")
ranked = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))
print("top:", ranked[:5])
user, n = ranked[0]
print(await submit_answer({"user": user, "requests": n}))
