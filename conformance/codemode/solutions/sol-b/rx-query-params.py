import re
from collections import Counter
from urllib.parse import urlsplit, parse_qsl
log = await get_access_log({"date": "2026-03-14"})
counts = Counter()
for line in log.splitlines():
    match = re.search(r'"\S+\s+(\S+)\s+HTTP/[^\"]+"', line)
    if match:
        names = {name for name, value in parse_qsl(urlsplit(match.group(1)).query, keep_blank_values=True)}
        counts.update(names)
await submit_answer({"params": dict(sorted(counts.items()))})
