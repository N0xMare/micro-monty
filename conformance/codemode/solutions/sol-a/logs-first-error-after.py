import re
from datetime import datetime

log = await fetch_logs({"service": "billing", "date": "2026-03-15"})
cutoff = datetime.fromisoformat("2026-03-15T06:00:00+00:00")
errors = []
for line in log.splitlines():
    match = re.match(r'^(\S+)\s+ERROR\s+billing\[\d+\]: (.*)$', line)
    if match and datetime.fromisoformat(match[1]) >= cutoff:
        errors.append((datetime.fromisoformat(match[1]), match[1], match[2]))
_, ts, message = min(errors, key=lambda item: item[0])
await submit_answer({"ts": ts, "message": message})
