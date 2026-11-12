import asyncio, re
from collections import Counter

dates = ("2026-03-13", "2026-03-14", "2026-03-15")
logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": day}) for day in dates))
paths = Counter()
for log in logs:
    for line in log.splitlines():
        path = re.search(r"\bpath=(\S+)", line)
        status = re.search(r"\bstatus=(\d{3})\b", line)
        if path and status and int(status.group(1)) >= 500:
            paths[path.group(1)] += 1
await submit_answer({"paths": dict(paths)})
