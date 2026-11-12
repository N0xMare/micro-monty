import re
from collections import Counter

text = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
total = Counter()
errors = Counter()
for line in text.splitlines():
    if "request completed" not in line:
        continue
    ts = re.match(r"(\d{4}-\d\d-\d\dT(\d\d):", line)
    status = re.search(r"\bstatus=(\d{3})\b", line)
    if not ts or not status:
        continue
    hour = ts.group(2)
    total[hour] += 1
    if status.group(1).startswith("5"):
        errors[hour] += 1
rates = {hour: round(100 * errors[hour] / n, 1) for hour, n in sorted(total.items())}
await submit_answer({"rates": rates})
