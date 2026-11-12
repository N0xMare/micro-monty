import re
from collections import Counter
counts = Counter()
for day in ("2026-03-13", "2026-03-14", "2026-03-15"):
    log = await fetch_logs({"service": "search", "date": day})
    for line in log.splitlines():
        status, path = re.search(r"\bstatus=(\d{3})\b", line), re.search(r"\bpath=([^\s]+)", line)
        if status and path and int(status.group(1)) >= 500: counts[path.group(1)] += 1
await submit_answer({"paths": dict(counts)})
