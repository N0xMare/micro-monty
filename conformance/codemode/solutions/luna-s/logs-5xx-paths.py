import asyncio
import re
from collections import Counter

dates = ("2026-03-13", "2026-03-14", "2026-03-15")
logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": date}) for date in dates))
counts = Counter()
for log in logs:
    for line in log.splitlines():
        status = re.search(r"\bstatus=(\d{3})\b", line)
        path = re.search(r"\bpath=([^\s]+)", line)
        if status and path and int(status.group(1)) >= 500:
            counts[path.group(1)] += 1
await submit_answer({"paths": dict(counts)})
