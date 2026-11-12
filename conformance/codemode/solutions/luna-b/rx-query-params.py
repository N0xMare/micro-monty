import re
from collections import Counter
from urllib.parse import urlsplit, parse_qsl

log = await get_access_log({"date": "2026-03-14"})
counts = Counter()
for line in log.splitlines():
    m = re.search(r'"\S+\s+(\S+)', line)
    if m:
        path = m.group(1)
        counts.update(set(key for key, _ in parse_qsl(urlsplit(path).query, keep_blank_values=True)))
await submit_answer({"params": dict(counts)})
