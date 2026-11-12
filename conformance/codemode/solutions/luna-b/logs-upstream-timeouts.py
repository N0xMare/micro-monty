import asyncio
import json
import re
from collections import Counter

services = json.loads(await list_services({}))
dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
calls = [(service, date) for service in services for date in dates]
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": date}) for service, date in calls))
timeouts = Counter()
for text in logs:
    for line in text.splitlines():
        match = re.search(r"upstream timeout calling ([^\s]+) after \d+ms", line)
        if match:
            timeouts[match.group(1)] += 1
await submit_answer({"timeouts": dict(timeouts)})
