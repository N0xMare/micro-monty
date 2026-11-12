import re
from collections import Counter

log = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
total = Counter()
errors = Counter()
for line in log.splitlines():
    if re.search(r"\b(?:request completed|completed request)\b", line):
        status = re.search(r"\bstatus=(\d{3})\b", line)
        if status:
            hour = line.split()[0][11:13]
            total[hour] += 1
            errors[hour] += 500 <= int(status[1]) < 600
await submit_answer({"rates": {hour: round(100 * errors[hour] / count, 1) for hour, count in sorted(total.items())}})
