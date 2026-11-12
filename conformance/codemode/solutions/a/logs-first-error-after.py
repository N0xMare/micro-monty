import re
from datetime import datetime, timezone

text = await fetch_logs({"service": "billing", "date": "2026-03-15"})
line_re = re.compile(r'^(\S+)\s+ERROR\s+billing\[\d+\]: (.*)$')
cutoff = datetime(2026, 3, 15, 6, 0, tzinfo=timezone.utc)
best = None
for line in text.splitlines():
    m = line_re.match(line.rstrip("\r"))
    if not m:
        continue
    ts = m.group(1)
    t = datetime.fromisoformat(ts.replace("Z", "+00:00"))
    if t >= cutoff and (best is None or t < best[0]):
        best = (t, ts, m.group(2))
print(best)
print(await submit_answer({"ts": best[1], "message": best[2]}))
