import re
from collections import defaultdict

text = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
completed, errors = defaultdict(int), defaultdict(int)
for line in text.splitlines():
    if "request completed" not in line:
        continue
    timestamp = line.split()[0]
    status_match = re.search(r"\bstatus=(\d{3})\b", line)
    if not status_match:
        continue
    hour, status = timestamp[11:13], int(status_match.group(1))
    completed[hour] += 1
    if 500 <= status < 600:
        errors[hour] += 1
rates = {hour: round(100 * errors[hour] / count, 1) for hour, count in sorted(completed.items())}
await submit_answer({"rates": rates})
