import re
from collections import Counter

log = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
total, failed = Counter(), Counter()
for line in log.splitlines():
    if "request completed" in line:
        match = re.search(r'\bstatus=(\d{3})\b', line)
        if match:
            hour = line[11:13]
            total[hour] += 1
            failed[hour] += 500 <= int(match[1]) < 600
await submit_answer({"rates": {hour: round(100 * failed[hour] / total[hour], 1) for hour in sorted(total)}})
