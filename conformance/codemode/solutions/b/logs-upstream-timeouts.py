import asyncio, json, re
from collections import Counter

services = json.loads(await list_services({}))
dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
pairs = [(s, d) for s in services for d in dates]
texts = await asyncio.gather(*(fetch_logs({"service": s, "date": d}) for s, d in pairs))
pat = re.compile(r"upstream timeout calling ([\w.-]+)")
timeouts = Counter()
for text in texts:
    for line in text.splitlines():
        m = pat.search(line)
        if m:
            timeouts[m.group(1)] += 1
print(dict(timeouts))
print(await submit_answer({"timeouts": dict(timeouts)}))
