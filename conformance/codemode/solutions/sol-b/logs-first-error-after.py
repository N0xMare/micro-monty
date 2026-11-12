import re
from datetime import datetime, timezone

log = await fetch_logs({"service": "billing", "date": "2026-03-15"})
cutoff = datetime(2026, 3, 15, 6, tzinfo=timezone.utc)
errors = []
for line in log.splitlines():
    match = re.match(r"(\S+)\s+ERROR\s+billing\[\d+\]: (.*)", line)
    if match:
        moment = datetime.fromisoformat(match[1])
        if moment >= cutoff:
            errors.append((moment, match[1], match[2]))
_, ts, message = min(errors, key=lambda item: item[0])
await submit_answer({"ts": ts, "message": message})
