import re
from collections import defaultdict

text = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
counts = defaultdict(lambda: [0, 0])
for line in text.splitlines():
    if "request" not in line.lower() or "completed" not in line.lower():
        continue
    ts = re.match(r"^(\d{4}-\d\d-\d\dT\d\d):", line)
    status = re.search(r"\bstatus=(\d{3})\b", line)
    if not ts or not status:
        continue
    counts[ts.group(1)[11:13]][0] += 1
    counts[ts.group(1)[11:13]][1] += status.group(1).startswith("5")
rates = {hour: round(100 * failures / total, 1) for hour, (total, failures) in sorted(counts.items()) if total}
await submit_answer({"rates": rates})
