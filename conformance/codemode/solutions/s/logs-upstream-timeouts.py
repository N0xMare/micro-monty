import asyncio
import re
from collections import Counter

services = await list_services({})
days = ["2026-03-13", "2026-03-14", "2026-03-15"]
pairs = [(s, d) for s in services for d in days]
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": d}) for s, d in pairs))
to_re = re.compile(r'upstream timeout calling (\S+?)(?=\s|$)')
timeouts = Counter()
for text in logs:
    for line in text.splitlines():
        m = to_re.search(line)
        if m:
            timeouts[m.group(1).strip('"\',:')] += 1
timeouts = dict(sorted(timeouts.items()))
print(timeouts)
print(await submit_answer({"timeouts": timeouts}))
