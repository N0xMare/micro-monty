import asyncio, json, re
from collections import Counter

services = json.loads(await list_services({}))
dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
pairs = [(s, d) for s in services for d in dates]
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": d}) for s, d in pairs))
to_re = re.compile(r'upstream timeout calling (\S+?) after \d+ms')
timeouts = Counter()
for text in logs:
    for line in text.splitlines():
        m = to_re.search(line)
        if m:
            timeouts[m.group(1)] += 1
timeouts = dict(sorted(timeouts.items(), key=lambda kv: (-kv[1], kv[0])))
print(timeouts)
print(await submit_answer({"timeouts": timeouts}))
