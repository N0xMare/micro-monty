import re
from urllib.parse import urlsplit, parse_qsl
from collections import Counter

log = await get_access_log({"date": "2026-03-14"})
counts = Counter()
for line in log.splitlines():
    request = re.search(r'"\S+\s+(\S+)\s+HTTP/[^" ]+"', line)
    if request:
        counts.update({name for name, value in parse_qsl(urlsplit(request[1]).query, keep_blank_values=True)})
await submit_answer({"params": dict(sorted(counts.items()))})
