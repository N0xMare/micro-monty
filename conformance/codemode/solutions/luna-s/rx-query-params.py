import re
from collections import Counter
from urllib.parse import urlsplit, parse_qsl

log = await get_access_log({"date": "2026-03-14"})
counts = Counter()
for line in log.splitlines():
    request = re.search(r'"\S+\s+(\S+)\s+HTTP/', line)
    if request:
        counts.update(set(key for key, _ in parse_qsl(urlsplit(request.group(1)).query, keep_blank_values=True)))
await submit_answer({"params": dict(sorted(counts.items()))})
