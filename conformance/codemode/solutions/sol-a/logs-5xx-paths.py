import asyncio
import re
from collections import Counter

logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": day}) for day in ("2026-03-13", "2026-03-14", "2026-03-15")))
paths = Counter()
for log in logs:
    for line in log.splitlines():
        fields = dict(re.findall(r'\b([A-Za-z_][A-Za-z0-9_]*)=("[^"\n]*"|\S+)', line))
        status = fields.get("status", "").strip('"')
        if status.isdigit() and int(status) >= 500 and "path" in fields:
            paths[fields["path"].strip('"')] += 1
await submit_answer({"paths": dict(paths)})
