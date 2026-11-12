import re
from urllib.parse import urlsplit, parse_qsl
from collections import Counter
log = await get_access_log({"date": "2026-03-14"})
terms = Counter()
for line in log.splitlines():
    match = re.search(r'"\S+\s+(\S+)\s+HTTP/[^" ]+"\s+\d{3}\b', line)
    if match:
        url = urlsplit(match.group(1))
        if url.path == "/search":
            terms.update(value for key, value in parse_qsl(url.query, keep_blank_values=True) if key == "q")
await submit_answer({"terms": dict(terms)})
