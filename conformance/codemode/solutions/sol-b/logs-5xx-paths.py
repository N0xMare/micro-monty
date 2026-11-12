import asyncio, re, shlex
from collections import Counter
logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": day}) for day in ("2026-03-13", "2026-03-14", "2026-03-15")))
paths = Counter()
for log in logs:
    for line in log.splitlines():
        fields = {}
        for token in shlex.split(line):
            if "=" in token:
                key, value = token.split("=", 1)
                fields[key] = value
        status = fields.get("status", "")
        if status.isdigit() and int(status) >= 500 and "path" in fields:
            paths[fields["path"]] += 1
await submit_answer({"paths": dict(paths)})
