import re
from urllib.parse import urlsplit, parse_qsl

log = await get_access_log({"date": "2026-03-14"})
counts = {}
for line in log.splitlines():
    m = re.search(r'"\S+\s+(\S+)', line)
    if not m:
        continue
    for key, _ in parse_qsl(urlsplit(m.group(1)).query, keep_blank_values=True):
        counts[key] = counts.get(key, 0) + 1
await submit_answer({"params": counts})
